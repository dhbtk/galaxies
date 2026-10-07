import { Description, Field, Label } from "@headlessui/react";
import { IMaskInput } from "react-imask";

export function MaskedField({
	name,
	label,
	hint,
	mask,
	value,
	onChange,
	error,
	onBlur,
}: {
	name: string;
	label: string;
	hint: string;
	mask: string;
	value: string;
	onChange: (value: string) => void;
	error?: string;
	onBlur: () => void;
}) {
	return (
		<Field className="form-field">
			<Label htmlFor={name}>{label}</Label>
			<IMaskInput
				id={name}
				name={name}
				type="text"
				inputMode="numeric"
				autoComplete="off"
				mask={mask}
				lazy={false}
				placeholderChar=" "
				value={value}
				onAccept={(next) => onChange(next)}
				onBlur={onBlur}
				aria-invalid={!!error}
				aria-describedby={`${name}-hint${error ? ` ${name}-error` : ""}`}
			/>
			<Description id={`${name}-hint`}>{hint}</Description>
			{error && (
				<p id={`${name}-error`} className="field-error">
					{error}
				</p>
			)}
		</Field>
	);
}
