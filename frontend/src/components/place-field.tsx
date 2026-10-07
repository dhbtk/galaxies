import {
	Combobox,
	ComboboxInput,
	ComboboxOption,
	ComboboxOptions,
	Field,
	Label,
} from "@headlessui/react";
import { useEffect, useState } from "react";
import { type Birthplace, searchPlaces } from "../lib/api";

function placeLabel(place: Birthplace) {
	return [place.name, place.admin1Name, place.countryCode]
		.filter(Boolean)
		.join(", ");
}

export function PlaceField({
	value,
	onChange,
	error,
}: {
	value: Birthplace | null;
	onChange: (place: Birthplace | null) => void;
	error?: string;
}) {
	const [query, setQuery] = useState("");
	const [places, setPlaces] = useState<Birthplace[]>([]);
	const [status, setStatus] = useState("");
	useEffect(() => {
		let active = true;
		setPlaces([]);
		if (!query.trim() || value) {
			setStatus("");
			return;
		}
		setStatus("Searching…");
		const timer = setTimeout(() => {
			searchPlaces({ data: query })
				.then((results) => {
					if (!active) return;
					setPlaces(results);
					setStatus(results.length ? "" : "No places found.");
				})
				.catch(() => {
					if (active)
						setStatus("Could not search places. Edit the search to try again.");
				});
		}, 250);
		return () => {
			active = false;
			clearTimeout(timer);
		};
	}, [query, value]);

	return (
		<Field className="form-field">
			<Label>Birth place</Label>
			<Combobox as="div" value={value} onChange={onChange} by="geonameId">
				<ComboboxInput
					autoComplete="off"
					placeholder="Search for a city"
					displayValue={(place: Birthplace | null) =>
						place ? placeLabel(place) : query
					}
					onChange={(event) => {
						onChange(null);
						setQuery(event.target.value);
					}}
					aria-invalid={!!error}
					aria-describedby={`place-status${error ? " place-error" : ""}`}
				/>
				<ComboboxOptions anchor="bottom start" className="place-options">
					{places.map((place) => (
						<ComboboxOption
							key={place.geonameId}
							value={place}
							className="place-option"
						>
							{placeLabel(place)}
						</ComboboxOption>
					))}
				</ComboboxOptions>
			</Combobox>
			<output id="place-status">{status}</output>
			{error && (
				<p id="place-error" className="field-error">
					{error}
				</p>
			)}
		</Field>
	);
}
