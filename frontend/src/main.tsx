import { createRouter, RouterProvider } from "@tanstack/react-router";
import ReactDOM from "react-dom/client";
import { routeTree } from "./routeTree.gen";

const router = createRouter({
	routeTree,
	defaultPreload: "intent",
	scrollRestoration: true,
});

declare module "@tanstack/react-router" {
	interface Register {
		router: typeof router;
	}
}

async function startDatasetDemo() {
	if (import.meta.env.DEV && import.meta.env.VITE_DATASET_DEMO === "true") {
		try {
			const { worker } = await import("./mocks/browser");
			await worker.start({ onUnhandledRequest: "bypass" });
		} catch (error) {
			console.error("Dataset demo worker failed to start", error);
		}
	}
}

const rootElement = document.getElementById("app");

if (rootElement && !rootElement.innerHTML) {
	await startDatasetDemo();
	const root = ReactDOM.createRoot(rootElement);
	root.render(<RouterProvider router={router} />);
}
