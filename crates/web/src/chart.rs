use chrono::{Datelike, Timelike};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use strum::IntoEnumIterator;
use strum_macros::EnumIter;
use sweph::Sign;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct BirthChart {
    pub latitude: f32,
    pub longitude: f32,
    pub time: chrono::DateTime<chrono::Utc>,
    pub asters: HashMap<Aster, AsterChartInfo>,
    pub rising: AsterChartInfo,
}

impl BirthChart {
    pub fn new(latitude: f32, longitude: f32, time: chrono::DateTime<chrono::Utc>) -> Self {
        let julian = sweph::utc_to_julian_day(
            time.year(),
            time.month(),
            time.day(),
            time.hour(),
            time.minute(),
            time.second() as f64,
        )
        .unwrap();
        let asters: HashMap<Aster, AsterChartInfo> = Aster::iter()
            .map(|aster| {
                let body = sweph::calc(julian.ut, aster.into()).unwrap();
                (
                    aster,
                    AsterChartInfo {
                        longitude: body.longitude as f32,
                        degrees: body.sign_degree() as f32,
                        sign: body.sign().into(),
                    },
                )
            })
            .collect();
        let houses = sweph::houses(
            julian.ut,
            latitude.into(),
            longitude as f64,
            sweph::HouseSystem::Placidus,
        )
        .unwrap();
        BirthChart {
            latitude,
            longitude,
            time,
            asters,
            rising: AsterChartInfo {
                longitude: houses.ascendant as f32,
                degrees: houses.ascendant as f32,
                sign: StarSign::from(houses.ascendant),
            },
        }
    }
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
