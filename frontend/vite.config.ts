import tailwindcss from "@tailwindcss/vite";
import { devtools } from "@tanstack/devtools-vite";

import { tanstackRouter } from "@tanstack/router-plugin/vite";

import viteReact from "@vitejs/plugin-react";
import { defineConfig } from "vitest/config";

const config = defineConfig({
	resolve: { tsconfigPaths: true },
	plugins: [
		devtools(),
		tailwindcss(),
		tanstackRouter({
			target: "react",
			autoCodeSplitting: true,
			routeFileIgnorePattern: "\\.test\\.tsx?$",
		}),
		viteReact(),
	],
	server: {
		proxy: {
			"/api": {
				target: process.env.VITE_PROXY_TARGET ?? "http://engine:3000",
				changeOrigin: true,
			},
		},
	},
	test: {
		globals: true,
		env: { VITE_API_URL: "http://localhost/api/v1" },
		environment: "jsdom",
		setupFiles: ["./src/test/setup.ts"],
		include: ["**/*.test.{ts,tsx}"],
		css: true,
	},
});

export default config;
