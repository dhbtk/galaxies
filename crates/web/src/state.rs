use sea_orm::{Database, DatabaseConnection};

#[derive(Clone)]
pub struct State {
    pub db: DatabaseConnection
}

impl State {
    pub async fn new() -> Self {
        let db = Database::connect("sqlite://catalog.sqlite").await.unwrap();
        Self { db }
    }
}
