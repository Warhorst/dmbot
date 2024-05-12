use std::env;
use serenity::all::*;
use serenity::async_trait;
use serenity::prelude::*;

#[tokio::main]
async fn main() {
    let token = env::var("DMBOT_TOKEN").expect("Expected a token in the environment");

    let intents = GatewayIntents::GUILD_MESSAGES // process messages for server (aka guild)
        | GatewayIntents::DIRECT_MESSAGES // process direct messages between a bot and a user
        | GatewayIntents::MESSAGE_CONTENT; // read message content (required for command processing)

    let mut client = Client::builder(&token, intents)
        .event_handler(Handler)
        .await
        .expect("Err creating client");

    if let Err(why) = client.start().await {
        println!("Client error: {why:?}");
    }
}

struct Handler;

#[async_trait]
impl EventHandler for Handler {
    /// called when the bot connects to a server
    async fn ready(&self, _ctx: Context, data_about_bot: Ready) {
        println!("{} is connected!", data_about_bot.user.name);
    }

    /// Process a message which might contain a command
    async fn message(&self, ctx: Context, message: Message) {
        match message.content.as_str() {
            "!join" => (),
            "!play" => (),
            "!leave" => (),
            _ => ()
        }
    }
}