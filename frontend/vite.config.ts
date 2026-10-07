import { devtools } from "@tanstack/devtools-vite";
import { tanstackStart } from "@tanstack/react-start/plugin/vite";
import viteReact from "@vitejs/plugin-react";
import { defineConfig } from "vite";

// Only the development/preview proxy knows the backend address.
const target = process.env.BACKEND_URL ?? "http://0.0.0.0:3000";
const proxy = {
	"/api": { target, changeOrigin: true },
	"/images": { target, changeOrigin: true },
};

const config = defineConfig({
	server: { proxy },
	preview: { proxy },
	resolve: { tsconfigPaths: true },
	plugins: [devtools(), tanstackStart(), viteReact()],
});

export default config;
