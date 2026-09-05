/** Per-request context handed to every page. */
import type { Env, SiteConfig } from "./env";

export interface Ctx {
	request: Request;
	url: URL;
	env: Env;
	cfg: SiteConfig;
	waitUntil: { waitUntil(p: Promise<unknown>): void };
}
