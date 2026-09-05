import { cloudflareTest } from "@cloudflare/vitest-pool-workers";
import { defineConfig } from "vitest/config";
import { handleBackendRequest } from "./test/backend-stub.ts";

export default defineConfig({
	plugins: [
		cloudflareTest({
			wrangler: { configPath: "./wrangler.jsonc" },
			miniflare: {
				bindings: {
					VI_API_ORIGIN: "https://backend.test",
					SITE_NAME: "VisionInjustice",
					CONTACT_EMAIL: "counsel@example.org",
					ENVIRONMENT: "test",
				},
				// Every outbound request is served by the stub backend: tests never
				// touch the network, and unexpected calls surface as 404s.
				outboundService: (request) =>
					handleBackendRequest(new Request(request.url, { method: request.method })),
			},
		}),
	],
	test: {
		include: ["test/**/*.test.ts"],
	},
});
