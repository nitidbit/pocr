import { act, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { EnvironmentProvider, useEnvironment } from "#/lib/environment";

const STORAGE_KEY = "pocr.environment";

// This test runner does not provide a real localStorage; back it with an
// in-memory Map so the provider's persistence path is actually exercised.
function installMemoryStorage() {
	const store = new Map<string, string>();
	const storage: Pick<Storage, "getItem" | "setItem" | "removeItem" | "clear"> =
		{
			getItem: (key) => store.get(key) ?? null,
			setItem: (key, value) => {
				store.set(key, value);
			},
			removeItem: (key) => {
				store.delete(key);
			},
			clear: () => {
				store.clear();
			},
		};
	vi.stubGlobal("localStorage", storage);
}

function Probe() {
	const { environment, setEnvironment } = useEnvironment();
	return (
		<div>
			<span data-testid="env">{environment}</span>
			<button type="button" onClick={() => setEnvironment("nonproduction")}>
				go non-prod
			</button>
		</div>
	);
}

function renderProbe() {
	return render(
		<EnvironmentProvider>
			<Probe />
		</EnvironmentProvider>,
	);
}

beforeEach(() => {
	installMemoryStorage();
});

afterEach(() => {
	vi.unstubAllGlobals();
});

describe("EnvironmentProvider", () => {
	it("defaults to production when no value is stored", () => {
		renderProbe();
		expect(screen.getByTestId("env")).toHaveTextContent("production");
	});

	it("falls back to production for a garbage stored value", () => {
		window.localStorage.setItem(STORAGE_KEY, "banana");
		renderProbe();
		expect(screen.getByTestId("env")).toHaveTextContent("production");
	});

	it("honors a valid stored non-production value", () => {
		window.localStorage.setItem(STORAGE_KEY, "nonproduction");
		renderProbe();
		expect(screen.getByTestId("env")).toHaveTextContent("nonproduction");
	});

	it("persists the selected environment to localStorage", () => {
		renderProbe();
		act(() => {
			screen.getByRole("button", { name: /go non-prod/i }).click();
		});
		expect(screen.getByTestId("env")).toHaveTextContent("nonproduction");
		expect(window.localStorage.getItem(STORAGE_KEY)).toBe("nonproduction");
	});
});
