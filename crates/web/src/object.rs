use crate::chart::StarSign;
use crate::state::State;
use axum::extract::FromRequestParts;
use axum::http;
use sea_orm::{
    ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QueryOrder, QuerySelect,
    sea_query::Expr,
};
use serde::{Deserialize, Serialize};
use std::convert::Infallible;

#[derive(Clone)]
pub struct ObjectRepository {
    db: DatabaseConnection,
}

impl FromRequestParts<State> for ObjectRepository {
    type Rejection = Infallible;

    async fn from_request_parts(
        _parts: &mut http::request::Parts,
        state: &State,
    ) -> Result<Self, Self::Rejection> {
        Ok(ObjectRepository {
            db: state.db.clone(),
        })
    }
}

/// A search target in equatorial degrees (RA in [0, 360), declination in [-90, 90]).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SearchTarget {
    pub ra_deg: f64,
    pub dec_deg: f64,
}

/// Product mapping range, not a physical limit on solar-system ecliptic latitude.
pub const DEFAULT_LATITUDE_HALF_WIDTH_DEG: f64 = 17.0;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CelestialObject {
    pub id: String,
    pub right_ascension_deg: f64,
    pub declination_deg: f64,
    pub object_type: String,
    pub morphology: Option<String>,
    pub image: ObjectImage,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ObjectImage {
    pub image_url: String,
    pub source_page: String,
    pub credit: String,
}

impl ObjectRepository {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }

    pub async fn find_at_sign_and_coordinates(
        &self,
        sign: StarSign,
        longitude: f64,
        latitude: f64,
    ) -> anyhow::Result<CelestialObject> {
        use crate::entity::images as Image;
        use crate::entity::objects as Object;
        let target = target_for_ecliptic(longitude, latitude)
            .ok_or_else(|| anyhow::anyhow!("invalid search coordinates"))?;
        let (object, image) = Object::Entity::find()
            .find_both_related(Image::Entity)
            .filter(Image::Column::SourceConstellation.eq(sign.constellation_name()))
            // SQLite's math extension is optional, so use only portable arithmetic.
            .order_by(
                squared_coordinate_distance_expr(target),
                sea_orm::Order::Asc,
            )
            // Make an exact tie deterministic.
            .order_by_asc(Object::Column::Id)
            .order_by_asc(Image::Column::Id)
            .limit(1)
            .one(&self.db)
            .await?
            .ok_or_else(|| anyhow::anyhow!("no image for {}", sign.constellation_name()))?;

        Ok(CelestialObject {
            id: object.id,
            right_ascension_deg: object.ra_deg,
            declination_deg: object.dec_deg,
            object_type: object.object_type,
            morphology: object.morphology,
            image: ObjectImage {
                image_url: image.local_path,
                source_page: image.source_page,
                credit: image.credit,
            },
        })
    }
}

/// Returns a portable, squared distance in equatorial degrees.
///
/// It is an approximation of angular separation, but handles the RA 0/360
/// boundary and uses only SQLite core functions.
fn squared_coordinate_distance_expr(target: SearchTarget) -> Expr {
    Expr::cust_with_values(
        "(\"objects\".\"dec_deg\" - ?) * (\"objects\".\"dec_deg\" - ?) + \
         (CASE WHEN abs(\"objects\".\"ra_deg\" - ?) > 180.0 \
               THEN 360.0 - abs(\"objects\".\"ra_deg\" - ?) \
               ELSE abs(\"objects\".\"ra_deg\" - ?) END) * \
         (CASE WHEN abs(\"objects\".\"ra_deg\" - ?) > 180.0 \
               THEN 360.0 - abs(\"objects\".\"ra_deg\" - ?) \
               ELSE abs(\"objects\".\"ra_deg\" - ?) END)",
        [
            target.dec_deg,
            target.dec_deg,
            target.ra_deg,
            target.ra_deg,
            target.ra_deg,
            target.ra_deg,
            target.ra_deg,
            target.ra_deg,
        ],
    )
}

/// Map a zodiac longitude and ecliptic latitude into the corresponding
/// constellation's J2000 bounding box. This is a symbolic mapping, not an
/// astronomical coordinate conversion or a guarantee of constellation membership.
/// Longitude must use the caller's intended zodiac (tropical or sidereal).
/// Latitude zero maps to the box midpoint; values beyond +/-17 degrees clamp.
/// Returns None for non-finite coordinates or latitude outside [-90, 90].
pub fn target_for_ecliptic(longitude: f64, latitude: f64) -> Option<SearchTarget> {
    target_for_ecliptic_with_band(longitude, latitude, DEFAULT_LATITUDE_HALF_WIDTH_DEG)
}

/// As above, with a configurable latitude half-width in (0, 90] degrees.
pub fn target_for_ecliptic_with_band(
    longitude: f64,
    latitude: f64,
    latitude_half_width_deg: f64,
) -> Option<SearchTarget> {
    if !longitude.is_finite()
        || !latitude.is_finite()
        || !(-90.0..=90.0).contains(&latitude)
        || !latitude_half_width_deg.is_finite()
        || latitude_half_width_deg <= 0.0
        || latitude_half_width_deg > 90.0
    {
        return None;
    }
    let longitude = longitude.rem_euclid(360.0);
    // rem_euclid can round a tiny negative input up to exactly 360.
    let longitude = if longitude >= 360.0 { 0.0 } else { longitude };
    let bounds = CONSTELLATION_BOUNDS[(longitude / 30.0) as usize];
    let along_sign = (longitude % 30.0) / 30.0;
    let along_latitude = (latitude / latitude_half_width_deg).clamp(-1.0, 1.0) * 0.5 + 0.5;
    Some(SearchTarget {
        ra_deg: (bounds.ra_start
            + along_sign * (bounds.ra_end - bounds.ra_start).rem_euclid(360.0))
        .rem_euclid(360.0),
        dec_deg: bounds.dec_min + along_latitude * (bounds.dec_max - bounds.dec_min),
    })
}
#[derive(Clone, Copy)]
struct Bounds {
    ra_start: f64,
    ra_end: f64,
    dec_min: f64,
    dec_max: f64,
}

// J2000 sampled boundary extrema, CDS VI/49 (Davenhall & Leggett), July 2024 revision:
// https://cdsarc.cds.unistra.fr/ftp/VI/49/ReadMe
// https://cdsarc.cds.unistra.fr/ftp/VI/49/bound_20.dat.gz
// Source RA and declination are both degrees. For each constellation, take
// min/max declination and the RA arc complementary to the largest circular gap
// between sorted boundary RAs. Thus Pisces crosses zero instead of spanning most
// of the sky. Ordered by the twelve zodiac signs; Scorpio -> SCO, Capricorn -> CAP.
// These are bounding rectangles, not the irregular IAU constellation polygons.
const CONSTELLATION_BOUNDS: [Bounds; 12] = [
    Bounds {
        ra_start: 26.655734,
        ra_end: 52.426668,
        dec_min: 10.3632069,
        dec_max: 31.2213154,
    }, // ARI
    Bounds {
        ra_start: 50.836683,
        ra_end: 90.228903,
        dec_min: -1.3461887,
        dec_max: 31.1003609,
    }, // TAU
    Bounds {
        ra_start: 90.125156,
        ra_end: 121.993231,
        dec_min: 9.8097754,
        dec_max: 35.390564,
    }, // GEM
    Bounds {
        ra_start: 118.832489,
        ra_end: 140.645985,
        dec_min: 6.4700689,
        dec_max: 33.1415138,
    }, // CNC
    Bounds {
        ra_start: 140.404259,
        ra_end: 179.608941,
        dec_min: -6.6916924,
        dec_max: 32.9691162,
    }, // LEO
    Bounds {
        ra_start: 174.342299,
        ra_end: 227.853012,
        dec_min: -22.6773415,
        dec_max: 14.3604937,
    }, // VIR
    Bounds {
        ra_start: 215.408506,
        ra_end: 240.571776,
        dec_min: -29.9948788,
        dec_max: -0.4742887,
    }, // LIB
    Bounds {
        ra_start: 236.813064,
        ra_end: 269.809284,
        dec_min: -45.7670517,
        dec_max: -8.2958899,
    }, // SCO
    Bounds {
        ra_start: 265.800191,
        ra_end: 307.169295,
        dec_min: -45.277565,
        dec_max: -11.6762342,
    }, // SGR
    Bounds {
        ra_start: 301.693696,
        ra_end: 329.770289,
        dec_min: -27.6419144,
        dec_max: -8.4043999,
    }, // CAP
    Bounds {
        ra_start: 309.579877,
        ra_end: 359.110564,
        dec_min: -24.9040413,
        dec_max: 3.3256676,
    }, // AQR
    Bounds {
        ra_start: 342.821416,
        ra_end: 31.665247,
        dec_min: -6.3074551,
        dec_max: 33.6818962,
    }, // PSC
];

#[cfg(test)]
mod tests {
    use super::*;

    fn close(actual: f64, expected: f64) {
        assert!((actual - expected).abs() < 1e-9, "{actual} != {expected}");
    }

    #[test]
    fn aries_interpolates_both_axes() {
        let target = target_for_ecliptic(15.0, 0.0).unwrap();
        close(target.ra_deg, (26.655734 + 52.426668) / 2.0);
        close(target.dec_deg, (10.3632069 + 31.2213154) / 2.0);
        close(
            target_for_ecliptic(0.0, -DEFAULT_LATITUDE_HALF_WIDTH_DEG)
                .unwrap()
                .dec_deg,
            10.3632069,
        );
        close(
            target_for_ecliptic(0.0, DEFAULT_LATITUDE_HALF_WIDTH_DEG)
                .unwrap()
                .dec_deg,
            31.2213154,
        );
    }

    #[test]
    fn sign_boundaries_and_longitude_wrap() {
        for (index, bounds) in CONSTELLATION_BOUNDS.iter().enumerate() {
            let longitude = index as f64 * 30.0;
            let target = target_for_ecliptic(longitude, 0.0).unwrap();
            close(target.ra_deg, bounds.ra_start);
            assert_eq!(Some(target), target_for_ecliptic(longitude + 720.0, 0.0));
            assert_eq!(Some(target), target_for_ecliptic(longitude - 360.0, 0.0));
        }
        close(target_for_ecliptic(345.0, 0.0).unwrap().ra_deg, 7.2433315);
        assert!(target_for_ecliptic(-f64::EPSILON, 0.0).is_some());
    }

    #[test]
    fn latitude_band_clamps_and_can_be_widened() {
        let half_width = DEFAULT_LATITUDE_HALF_WIDTH_DEG;
        assert_eq!(
            target_for_ecliptic(15.0, 90.0),
            target_for_ecliptic(15.0, half_width)
        );
        assert_eq!(
            target_for_ecliptic(15.0, -90.0),
            target_for_ecliptic(15.0, -half_width)
        );
        let target = target_for_ecliptic_with_band(15.0, 10.0, 20.0).unwrap();
        // +10 in a +/-20 degree band is three quarters from bottom to top.
        close(
            target.dec_deg,
            10.3632069 + 0.75 * (31.2213154 - 10.3632069),
        );
        close(
            target_for_ecliptic(15.0, half_width / 2.0).unwrap().dec_deg,
            target.dec_deg,
        );
    }

    #[test]
    fn rejects_invalid_inputs() {
        for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert!(target_for_ecliptic(value, 0.0).is_none());
            assert!(target_for_ecliptic(0.0, value).is_none());
        }
        for value in [-91.0, 91.0] {
            assert!(target_for_ecliptic(0.0, value).is_none());
        }
        for band in [0.0, -10.0, 91.0, f64::NAN, f64::INFINITY] {
            assert!(target_for_ecliptic_with_band(0.0, 0.0, band).is_none());
        }
    }
}
