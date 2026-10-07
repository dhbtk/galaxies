import { Temporal } from "@js-temporal/polyfill";

export function parseBirthDate(value: string) {
	if (!/^\d{2}\/\d{2}\/\d{4}$/.test(value))
		throw new Error("Enter a date as DD/MM/YYYY.");
	const [day, month, year] = value.split("/").map(Number);
	if (year < 1) throw new Error("Enter a valid year.");
	return Temporal.PlainDate.from({ year, month, day }, { overflow: "reject" });
}

export function parseBirthTime(value: string) {
	if (!/^\d{2}:\d{2}$/.test(value)) throw new Error("Enter a time as HH:mm.");
	const [hour, minute] = value.split(":").map(Number);
	return Temporal.PlainTime.from({ hour, minute }, { overflow: "reject" });
}

export function birthTimeToUtc(date: string, time: string, timezone: string) {
	return parseBirthDate(date)
		.toPlainDateTime(parseBirthTime(time))
		.toZonedDateTime(timezone, { disambiguation: "reject" })
		.toInstant()
		.toString();
}
