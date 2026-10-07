import { createIsomorphicFn } from "@tanstack/react-start";
import { getRequestUrl } from "@tanstack/react-start/server";
import type { BirthChartResponse } from "./chart";

export interface Birthplace {
	geonameId: number;
	name: string;
	admin1Name: string | null;
	countryCode: string;
	latitude: number;
	longitude: number;
	timezone: string;
}

// Browsers use relative URLs; SSR resolves the same paths against the request origin.
const apiUrl = createIsomorphicFn()
	.client((path: string) => path)
	.server((path: string) => new URL(path, getRequestUrl()).href);

async function backendGet<T>(path: string): Promise<T> {
	const response = await fetch(apiUrl(path), {
		signal: AbortSignal.timeout(15000),
	});
	if (!response.ok)
		throw new Error(`Backend request failed (${response.status}).`);
	return response.json();
}

export async function searchPlaces({
	data: query,
}: {
	data: string;
}): Promise<Birthplace[]> {
	const search = query.trim();
	if (search.length > 200) throw new Error("Invalid place search.");
	return search
		? backendGet<Birthplace[]>(`/api/v1/search/${encodeURIComponent(search)}`)
		: [];
}

export async function loadChart({
	data: params,
}: {
	data: { latitude: string; longitude: string; time: string };
}): Promise<BirthChartResponse> {
	if (
		!params.latitude.trim() ||
		!params.longitude.trim() ||
		!Number.isFinite(Number(params.latitude)) ||
		Math.abs(Number(params.latitude)) > 90 ||
		!Number.isFinite(Number(params.longitude)) ||
		Math.abs(Number(params.longitude)) > 180 ||
		!Number.isFinite(Date.parse(params.time))
	)
		throw new Error("Invalid chart coordinates or time.");
	return backendGet<BirthChartResponse>(
		`/api/v1/chart/${encodeURIComponent(params.latitude)}/${encodeURIComponent(params.longitude)}/${encodeURIComponent(params.time)}`,
	);
}
