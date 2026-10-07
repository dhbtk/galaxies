use std::convert::Infallible;
use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use sea_orm::{raw_sql, DatabaseConnection, FromQueryResult};
use serde::{Deserialize, Serialize};
use crate::state::State;

#[derive(Clone)]
pub struct BirthplaceRepository {
    db: DatabaseConnection,
}

impl FromRequestParts<State> for BirthplaceRepository {
    type Rejection = Infallible;

    async fn from_request_parts(_parts: &mut Parts, state: &State) -> Result<Self, Self::Rejection> {
        Ok(Self {
            db: state.db.clone(),
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, FromQueryResult)]
#[serde(rename_all = "camelCase")]
pub struct Birthplace {
    pub geoname_id: i32,
    pub name: String,
    pub admin1_name: Option<String>,
    pub country_code: String,
    pub latitude: f64,
    pub longitude: f64,
    pub timezone: String,
}

impl BirthplaceRepository {
    pub async fn search(&self, query: String) -> anyhow::Result<Vec<Birthplace>> {
        let match_value = to_sqlite_match_query(query);
        let result: Vec<Birthplace> = Birthplace::find_by_statement(raw_sql!(
            Sqlite,
            r#"
SELECT rowid AS geoname_id, name, admin1_name, country_code,
       latitude, longitude, timezone
FROM birthplaces
WHERE birthplaces MATCH {match_value}
ORDER BY population DESC, rowid
LIMIT 10
            "#
        ))
            .all(&self.db).await?;
        Ok(result)
    }
}

fn to_sqlite_match_query(input: String) -> String {
    let words = input.split_whitespace().map(|i| format!("\"{}\"", i.replace("\"", "\\\""))).collect::<Vec<_>>();
    format!("{}*", words.join(" "))
}
