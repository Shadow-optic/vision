/** Binds `cloudflare:test`'s `env` to the Worker's own binding contract. */
declare namespace Cloudflare {
	interface Env extends import("../worker/env").Env {}
}
