mod database;
mod ytdlp;

use reqwest::Client as HttpClient;
use serenity::all::*;
use serenity::async_trait;
use serenity::framework::standard::macros::{command, group};
use serenity::framework::standard::{Args, CommandResult, Configuration};
use serenity::prelude::*;
use songbird::input::{Compose, YoutubeDl};
use songbird::{EventContext, SerenityInit, TrackEvent};
use std::env;
use std::sync::Arc;
use itertools::Itertools;
use crate::database::Database;
use crate::ytdlp::get_video_name;
// see https://github.com/serenity-rs/serenity/blob/current/examples/e01_basic_ping_bot/src/main.rs
// see https://github.com/serenity-rs/songbird/blob/current/examples/serenity/voice/src/main.rs

#[tokio::main]
async fn main() {
    let token = env::var("DMBOT_TOKEN").expect("Expected a token in the environment");

    let framework = StandardFramework::new().group(&DMBOT_GROUP);
    framework.configure(Configuration::new().prefix("!"));

    let intents = GatewayIntents::non_privileged() | GatewayIntents::MESSAGE_CONTENT;

    let mut client = Client::builder(&token, intents)
        .event_handler(Handler)
        .framework(framework)
        .register_songbird()
        .type_map_insert::<HttpKey>(HttpClient::new())
        .type_map_insert::<DbKey>(Arc::new(Mutex::new(Database::open())))
        .await
        .expect("Err creating client");

    if let Err(why) = client.start().await {
        println!("Client error: {why:?}");
    }
}

/// Key to access the client stored in the type map. The client is used to play YouTube tracks.
struct HttpKey;

impl TypeMapKey for HttpKey {
    type Value = HttpClient;
}

struct DbKey;

impl TypeMapKey for DbKey {
    type Value = Arc<Mutex<Database>>;
}

struct Handler;

#[async_trait]
impl EventHandler for Handler {
    /// called when the bot connects to a server
    async fn ready(&self, _ctx: Context, data_about_bot: Ready) {
        println!("{} is connected!", data_about_bot.user.name);
    }
}

struct TrackErrorNotifier;

/// processes errors which might occur when playing music tracks with songbird
#[async_trait]
impl songbird::events::EventHandler for TrackErrorNotifier {
    async fn act(&self, ctx: &EventContext<'_>) -> Option<songbird::Event> {
        if let EventContext::Track(track_list) = ctx {
            for (state, handle) in *track_list {
                println!(
                    "Track {:?} encountered an error: {:?}",
                    handle.uuid(),
                    state.playing
                );
            }
        }

        None
    }
}

/// All commands the bot supports
#[group]
#[commands(play, reg, skip, stop, help)]
struct DMBot;

/// Main command which is used to join a channel and play some music from YouTube.
#[command]
#[only_in(guilds)]
async fn play(
    context: &Context,
    message: &Message,
    mut args: Args,
) -> CommandResult {
    let input = args.iter::<String>().map(|r| r.unwrap()).collect::<Vec<_>>().join(" ");

    let (guild_id, channel_id) = get_guild_and_voice_channel(context, message);
    let connect_to = match channel_id {
        Some(channel) => channel,
        None => {
            check_msg(message.reply(context, "Not in a voice channel").await);
            return Ok(());
        }
    };

    let manager = songbird::get(&context)
        .await
        .expect("Songbird Voice client placed in at initialisation.")
        .clone();

    if let Ok(handler_lock) = manager.join(guild_id, connect_to).await {
        // Attach an event handler to see notifications of all track errors.
        let mut handler = handler_lock.lock().await;
        handler.add_global_event(TrackEvent::Error.into(), TrackErrorNotifier);
    }

    let http_client = {
        let data = context.data.read().await;
        data.get::<HttpKey>()
            .cloned()
            .expect("The HTTP client should exist in the type map.")
    };

    let database = {
        let data = context.data.read().await;
        data.get::<DbKey>()
            .cloned()
            .expect("The database should exist in the type map")
    };

    if let Some(handler_lock) = manager.get(guild_id) {
        let mut handler = handler_lock.lock().await;

        let url = match input.starts_with("https") {
            true => input,
            false => {
                let videos = database.lock().await.find_videos_like(input);

                match videos.len() {
                    0 => {
                        check_msg(message.channel_id.say(&context.http, format!("No videos with a name like this exist")).await);
                        return Ok(())
                    },
                    1 => format!("https://www.youtube.com/watch?v={}", videos.get(0).unwrap().clone().0),
                    _ => {
                        let videos_string = videos.iter().map(|(_, title)| title).join(", ");
                        check_msg(message.channel_id.say(&context.http, format!("More than one video was found: {videos_string}. Be more specific")).await);
                        return Ok(())
                    }
                }
            }
        };

        let mut src = YoutubeDl::new(http_client, url);
        let _ = handler.enqueue_input(src.clone().into()).await;

        let queue_position = handler.queue().len();
        let title = src.aux_metadata().await.unwrap().title.unwrap();

        check_msg(message.channel_id.say(&context.http, format!("Added '{title}' in queue position {queue_position}")).await);
    } else {
        check_msg(
            message.channel_id
                .say(&context.http, "Not in a voice channel to play in")
                .await,
        );
    }

    Ok(())
}

/// Used to register a song by storing its YouTube id and name in the dmbot database.
#[command]
#[only_in(guilds)]
async fn reg(
    context: &Context,
    message: &Message,
    mut args: Args,
) -> CommandResult {
    let url = match args.single::<String>() {
        Ok(url) => url,
        Err(_) => {
            check_msg(
                message.channel_id
                    .say(&context.http, "Must provide a URL to a video or audio")
                    .await,
            );

            return Ok(());
        }
    };

    let title = match get_video_name(&url) {
        Ok(title) => title,
        Err(e) => {
            check_msg(message.channel_id.say(&context.http, format!("Could not retrieve video name. {e}")).await);
            return Ok(())
        }
    };

    let database = {
        let data = context.data.read().await;
        data.get::<DbKey>()
            .cloned()
            .expect("The database should exist in the type map")
    };

    let raw_id = {
        let mut id = url.replace("https://www.youtube.com/watch?v=", "");
        id.split("&").next().unwrap().into()
    };

    if let Err(e) = database.lock().await.add_song(raw_id, title) {
        check_msg(message.channel_id.say(&context.http, format!("Could not store video in database. {e}")).await);
        return Ok(())
    }

    check_msg(message.channel_id.say(&context.http, "Video registered in database.").await);

    Ok(())
}

/// stop the current song and go to the next one in the queue
#[command]
#[only_in(guilds)]
async fn skip(
    context: &Context,
    message: &Message,
    _args: Args,
) -> CommandResult {
    let guild_id = message.guild_id.unwrap();

    let manager = songbird::get(context)
        .await
        .expect("Songbird Voice client placed in at initialisation.")
        .clone();

    if let Some(handler_lock) = manager.get(guild_id) {
        let handler = handler_lock.lock().await;
        let queue = handler.queue();
        let _ = queue.skip();

        let answer = match queue.len() {
            0 => "Skipping current song. The queue is now empty.",
            _ => "Skipping current song"
        }.to_string();

        check_msg(message.channel_id.say(
            &context.http,
            answer,
        ).await);
    }

    Ok(())
}

/// stop the current song and clear the queue
#[command]
#[only_in(guilds)]
async fn stop(
    context: &Context,
    message: &Message,
    _args: Args,
) -> CommandResult {
    let guild_id = message.guild_id.unwrap();

    let manager = songbird::get(context)
        .await
        .expect("Songbird Voice client placed in at initialisation.")
        .clone();

    if let Some(handler_lock) = manager.get(guild_id) {
        let handler = handler_lock.lock().await;
        let queue = handler.queue();
        let _ = queue.stop();

        check_msg(message.channel_id.say(
            &context.http,
            "Current song stopped and queue cleared.".to_string(),
        ).await);
    }

    Ok(())
}

/// display a help message
#[command]
#[only_in(guilds)]
async fn help(
    context: &Context,
    message: &Message,
    _args: Args,
) -> CommandResult {
    let mut help_message = String::new();
    help_message += "!help = show this message";
    help_message += "\n";
    help_message += "!play <YouTube URL> = add the given Youtube link to the queue";
    help_message += "\n";
    help_message += "!skip = skip the currently playing song and go to the next one in the queue";
    help_message += "\n";
    help_message += "!stop = stop the current song and clear the queue";

    check_msg(message.channel_id.say(&context.http, help_message).await);
    Ok(())
}

fn get_guild_and_voice_channel(context: &Context, message: &Message) -> (GuildId, Option<ChannelId>) {
    let guild = message.guild(&context.cache).unwrap();
    let channel_id = guild
        .voice_states
        .get(&message.author.id)
        .and_then(|voice_state| voice_state.channel_id);

    (guild.id, channel_id)
}

/// Checks that a message successfully sent; if not, then logs why to stdout.
fn check_msg(result: Result<Message>) {
    if let Err(why) = result {
        println!("Error sending message: {:?}", why);
    }
}