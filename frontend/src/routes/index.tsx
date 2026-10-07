import { createFileRoute } from "@tanstack/react-router";
import { type FormEvent, useState } from "react";
import { MaskedField } from "../components/masked-field";
import { PlaceField } from "../components/place-field";
import type { Birthplace } from "../lib/api";
import {
	birthTimeToUtc,
	parseBirthDate,
	parseBirthTime,
} from "../lib/birth-time";

export const Route = createFileRoute("/")({ component: Home });

function Home() {
	const navigate = Route.useNavigate();
	const [date, setDate] = useState("");
	const [time, setTime] = useState("");
	const [place, setPlace] = useState<Birthplace | null>(null);
	const [touched, setTouched] = useState({
		date: false,
		time: false,
		place: false,
	});
	const [error, setError] = useState("");
	const [submitting, setSubmitting] = useState(false);
	let dateError = "",
		timeError = "";
	try {
		parseBirthDate(date);
	} catch {
		dateError = "Enter a valid date as DD/MM/YYYY.";
	}
	try {
		parseBirthTime(time);
	} catch {
		timeError = "Enter a valid 24-hour time as HH:mm.";
	}

	async function submit(event: FormEvent) {
		event.preventDefault();
		setTouched({ date: true, time: true, place: true });
		setError("");
		if (dateError || timeError || !place) return;
		let utc: string;
		try {
			utc = birthTimeToUtc(date, time, place.timezone);
		} catch {
			setError(
				"This local time cannot be resolved uniquely in the selected timezone. It may fall in a daylight-saving clock change. Please check the date, time, and place.",
			);
			return;
		}
		setSubmitting(true);
		try {
			await navigate({
				to: "/chart/$latitude/$longitude/$time",
				params: {
					latitude: String(place.latitude),
					longitude: String(place.longitude),
					time: utc,
				},
			});
		} catch {
			setError("Could not open the chart. Please try again.");
		} finally {
			setSubmitting(false);
		}
	}

	return (
		<main>
			<h1>Birth chart</h1>
			<form onSubmit={submit} noValidate className="birth-form">
				<MaskedField
					name="birth-date"
					label="Birth date"
					hint="DD/MM/YYYY"
					mask="00/00/0000"
					value={date}
					onChange={setDate}
					onBlur={() => setTouched((state) => ({ ...state, date: true }))}
					error={touched.date ? dateError : undefined}
				/>
				<MaskedField
					name="birth-time"
					label="Birth time"
					hint="HH:mm (24-hour)"
					mask="00:00"
					value={time}
					onChange={setTime}
					onBlur={() => setTouched((state) => ({ ...state, time: true }))}
					error={touched.time ? timeError : undefined}
				/>
				<PlaceField
					value={place}
					onChange={setPlace}
					error={
						touched.place && !place
							? "Select a birth place from the suggestions."
							: undefined
					}
				/>
				{error && (
					<p role="alert" className="field-error">
						{error}
					</p>
				)}
				<button type="submit" disabled={submitting}>
					{submitting ? "Loading chart…" : "View chart"}
				</button>
			</form>
		</main>
	);
}
