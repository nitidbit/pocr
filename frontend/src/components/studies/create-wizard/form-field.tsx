import { cloneElement, isValidElement, type ReactElement } from "react";

interface FormFieldProps {
	label: string;
	name: string;
	required?: boolean;
	error?: string;
	children: React.ReactNode;
}

/** Reusable form field wrapper with label, required indicator, and error display. */
export function FormField({
	label,
	name,
	required,
	error,
	children,
}: FormFieldProps) {
	const errorId = `${name}-error`;

	const enhancedChild = isValidElement(children)
		? cloneElement(children as ReactElement<Record<string, unknown>>, {
				id: name,
				...(required && { "aria-required": "true" }),
				...(error && { "aria-describedby": errorId }),
			})
		: children;

	return (
		<div className="flex flex-col gap-1.5">
			<label htmlFor={name} className="text-sm font-medium text-foreground">
				{label}
				{required && (
					<span className="ml-0.5 text-destructive" aria-hidden="true">
						*
					</span>
				)}
			</label>
			{enhancedChild}
			{error && (
				<p id={errorId} className="text-xs text-destructive" role="alert">
					{error}
				</p>
			)}
		</div>
	);
}
