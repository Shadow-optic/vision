import { DurableObject } from "cloudflare:workers";

/** Kept so the existing Cloudflare Worker `vision` retains its v1 SQLite DO class. */
export class WorkflowStatusDO extends DurableObject {
	async fetch(): Promise<Response> {
		return new Response("ok");
	}
}
