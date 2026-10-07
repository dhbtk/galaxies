use chrono::{Datelike, Timelike};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::convert::Infallible;
use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use strum::IntoEnumIterator;
use strum_macros::EnumIter;
use sweph::Sign;
use crate::repository::{CelestialObject, ObjectImage, ObjectRepository};
use crate::state::State;

#[derive(Clone)]
pub struct BirthChartRepository {
    objects: ObjectRepository
}

impl FromRequestParts<State> for BirthChartRepository {
    type Rejection = Infallible;

    async fn from_request_parts(_parts: &mut Parts, state: &State) -> Result<Self, Self::Rejection> {
        let objects = ObjectRepository::new(state.db.clone());
        Ok(Self::new(objects))
    }
}

impl BirthChartRepository {
    pub fn new(objects: ObjectRepository) -> Self {
        Self { objects }
    }

    pub async fn calculate(&self, latitude: f32, longitude: f32, time: chrono::DateTime<chrono::Utc>) -> anyhow::Result<BirthChart> {
        let julian = sweph::utc_to_julian_day(
            time.year(),
            time.month(),
            time.day(),
            time.hour(),
            time.minute(),
            time.second() as f64,
        )?;
        let mut asters = HashMap::new();
        for aster in Aster::iter() {
            let body = sweph::calc(julian.ut, aster.into())?;
            asters.insert(aster, AsterChartInfo {
                longitude: body.longitude as f32,
                degrees: body.sign_degree() as f32,
                sign: body.sign().into(),
                object: self.objects.find_at_sign_and_coordinates(body.sign().into(), body.longitude, body.latitude).await?,
            });
        }
        let houses = sweph::houses(
            julian.ut,
            latitude.into(),
            longitude as f64,
            sweph::HouseSystem::Placidus,
        )?;
        Ok(BirthChart {
            latitude,
            longitude,
            time,
            asters,
            rising: AsterChartInfo {
                longitude: houses.ascendant as f32,
                degrees: houses.ascendant as f32,
                sign: StarSign::from(houses.ascendant),
                object: self.objects.find_at_sign_and_coordinates(StarSign::from(houses.ascendant), houses.ascendant, latitude as f64).await?,
            },
        })
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct BirthChart {
    pub latitude: f32,
    pub longitude: f32,
    pub time: chrono::DateTime<chrono::Utc>,
    pub asters: HashMap<Aster, AsterChartInfo>,
    pub rising: AsterChartInfo,
}

#[derive(Serialize, Deserialize, EnumIter, Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub enum Aster {
    Sun,
    Moon,
    Mercury,
    Venus,
    Mars,
    Jupiter,
    Saturn,
    Neptune,
    Uranus,
    Pluto,
    Lilith,
}

impl From<Aster> for sweph::Body {
    fn from(value: Aster) -> Self {
        match value {
            Aster::Sun => sweph::Body::Sun,
            Aster::Moon => sweph::Body::Moon,
            Aster::Mercury => sweph::Body::Mercury,
            Aster::Venus => sweph::Body::Venus,
            Aster::Mars => sweph::Body::Mars,
            Aster::Jupiter => sweph::Body::Jupiter,
            Aster::Saturn => sweph::Body::Saturn,
            Aster::Neptune => sweph::Body::Neptune,
            Aster::Uranus => sweph::Body::Uranus,
            Aster::Pluto => sweph::Body::Pluto,
            Aster::Lilith => sweph::Body::MeanApogee,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AsterChartInfo {
    pub longitude: f32,
    pub degrees: f32,
    pub sign: StarSign,
    pub object: CelestialObject,
}

#[derive(Serialize, Deserialize, Debug, Clone, Eq, PartialEq, Hash)]
pub enum StarSign {
    Aries,
    Taurus,
    Gemini,
    Cancer,
    Leo,
    Virgo,
    Libra,
    Scorpio,
    Sagittarius,
    Capricorn,
    Aquarius,
    Pisces,
}

impl StarSign {
    pub fn constellation_name(&self) -> &str {
        match self {
            StarSign::Aries => "Aries",
            StarSign::Taurus => "Taurus",
            StarSign::Gemini => "Gemini",
            StarSign::Cancer => "Cancer",
            StarSign::Leo => "Leo",
            StarSign::Virgo => "Virgo",
            StarSign::Libra => "Libra",
            StarSign::Scorpio => "Scorpius",
            StarSign::Sagittarius => "Sagittarius",
            StarSign::Capricorn => "Capricornus",
            StarSign::Aquarius => "Aquarius",
            StarSign::Pisces => "Pisces",
        }
    }
}

impl From<sweph::Sign> for StarSign {
    fn from(value: sweph::Sign) -> Self {
        match value {
            Sign::Aries => StarSign::Aries,
            Sign::Taurus => StarSign::Taurus,
            Sign::Gemini => StarSign::Gemini,
            Sign::Cancer => StarSign::Cancer,
            Sign::Leo => StarSign::Leo,
            Sign::Virgo => StarSign::Virgo,
            Sign::Libra => StarSign::Libra,
            Sign::Scorpio => StarSign::Scorpio,
            Sign::Sagittarius => StarSign::Sagittarius,
            Sign::Capricorn => StarSign::Capricorn,
            Sign::Aquarius => StarSign::Aquarius,
            Sign::Pisces => StarSign::Pisces,
        }
    }
}

impl From<f64> for StarSign {
    fn from(value: f64) -> Self {
        StarSign::from(sweph::Sign::from_longitude(value))
    }
}
