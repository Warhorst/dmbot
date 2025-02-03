use rusqlite::{Connection, Error};
use std::path::PathBuf;

pub struct Database(Connection);

impl Database {
    pub fn open() -> Self {
        let connection = Connection::open(Self::get_path()).expect("Failed to open db");

        connection.execute("\
        CREATE TABLE IF NOT EXISTS Songs (
            video_id TEXT PRIMARY KEY,
            video_title TEXT
        )
        ", []).expect("Failed to create songs table");

        Database(connection)
    }

    fn get_path() -> PathBuf {
        let mut path = std::env::current_exe().unwrap();
        path.pop();
        path.push("dmbot.db");
        path
    }

    /// Add the given video id and title to the database
    pub fn add_song(
        &self,
        video_id: String,
        video_title: String
    ) -> Result<(), String> {
         self.0.execute("\
            INSERT INTO Songs (video_id, video_title) VALUES (?1, ?2);
        ", &[&video_id, &video_title]).map_err(Self::map_db_error)?;

        Ok(())
    }

    pub fn find_videos_like(&self, input: String) -> Vec<(String, String)> {
        match self.get_all_videos_and_titles() {
            Ok(iter) => iter
                .into_iter()
                .filter(|(_, title)| title.to_lowercase().contains(&input.to_lowercase()))
                .collect(),
            Err(_) => vec![]
        }
    }

    fn get_all_videos_and_titles(&self) -> Result<Vec<(String, String)>, String> {
        let mut statement = self.0.prepare("SELECT * FROM Songs").map_err(Self::map_db_error)?;

        let result = statement.query_map([], |row| Ok((
            row.get(0).unwrap(),
            row.get(1).unwrap()
        ))).map_err(Self::map_db_error)?;

        Ok(result.map(|r| r.unwrap()).collect())
    }

    fn map_db_error(error: Error) -> String {
        format!("Failed to get videos due to error: {:?}", error)
    }
}