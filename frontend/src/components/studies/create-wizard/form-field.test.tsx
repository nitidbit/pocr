import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { FormField } from "./form-field";

describe("FormField", () => {
	it("associates the label with the input via accessible name", () => {
		render(
			<FormField label="Study Title" name="title">
				<input />
			</FormField>,
		);

		expect(
			screen.getByRole("textbox", { name: "Study Title" }),
		).toBeInTheDocument();
	});

	it("marks input as aria-required when required is true", () => {
		render(
			<FormField label="IRB Number" name="irbNumber" required>
				<input />
			</FormField>,
		);

		expect(screen.getByRole("textbox", { name: /IRB Number/ })).toHaveAttribute(
			"aria-required",
			"true",
		);
	});

	it("shows visual required indicator when required is true", () => {
		render(
			<FormField label="IRB Number" name="irbNumber" required>
				<input />
			</FormField>,
		);

		expect(screen.getByText("*")).toBeInTheDocument();
	});

	it("does not mark input as aria-required when required is false", () => {
		render(
			<FormField label="Description" name="description">
				<input />
			</FormField>,
		);

		expect(
			screen.getByRole("textbox", { name: "Description" }),
		).not.toHaveAttribute("aria-required");
	});

	it("renders children within the field", () => {
		render(
			<FormField label="Protocol ID" name="protocolId">
				<input placeholder="Enter protocol ID" />
			</FormField>,
		);

		expect(
			screen.getByPlaceholderText("Enter protocol ID"),
		).toBeInTheDocument();
	});

	it("associates error message with input via aria-describedby", () => {
		render(
			<FormField label="Study Title" name="title" error="Title is required">
				<input />
			</FormField>,
		);

		expect(
			screen.getByRole("textbox", { name: "Study Title" }),
		).toHaveAccessibleDescription("Title is required");
	});

	it("does not render error text when no error prop is provided", () => {
		render(
			<FormField label="Study Title" name="title">
				<input />
			</FormField>,
		);

		expect(screen.queryByRole("alert")).not.toBeInTheDocument();
	});
});
