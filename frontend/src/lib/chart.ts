/** JSON contract from crates/web/src/chart.rs, object.rs, and main.rs. */
export type Aster =
	| "Sun"
	| "Moon"
	| "Mercury"
	| "Venus"
	| "Mars"
	| "Jupiter"
	| "Saturn"
	| "Neptune"
	| "Uranus"
	| "Pluto"
	| "Lilith";

export type StarSign =
	| "Aries"
	| "Taurus"
	| "Gemini"
	| "Cancer"
	| "Leo"
	| "Virgo"
	| "Libra"
	| "Scorpio"
	| "Sagittarius"
	| "Capricorn"
	| "Aquarius"
	| "Pisces";

export interface ObjectImage {
	imageUrl: string;
	sourcePage: string;
	credit: string;
}

export interface CelestialObject {
	id: string;
	rightAscensionDeg: number;
	declinationDeg: number;
	objectType: string;
	morphology: string | null;
	image: ObjectImage;
}

export interface AsterChartInfo {
	longitude: number;
	degrees: number;
	sign: StarSign;
	object: CelestialObject;
}

export interface BirthChart {
	latitude: number;
	longitude: number;
	/** RFC 3339 timestamp in UTC, serialized by the backend. */
	time: string;
	/** The backend calculates every Aster for each successful response. */
	asters: Record<Aster, AsterChartInfo>;
	rising: AsterChartInfo;
}

export interface BirthChartResponse {
	chart: BirthChart;
}

export type DisplayableAster = Aster | 'Rising';
