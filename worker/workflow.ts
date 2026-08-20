import {
	WorkflowEntrypoint,
	type WorkflowEvent,
	type WorkflowStep,
} from "cloudflare:workers";

type WorkerEnv = {
	MY_WORKFLOW: Workflow;
	WORKFLOW_STATUS: DurableObjectNamespace;
};

/** Kept so the existing Cloudflare Worker `vision` retains its Workflow binding. */
export class MyWorkflow extends WorkflowEntrypoint<
	WorkerEnv,
	Record<string, unknown>
> {
	async run(
		_event: WorkflowEvent<Record<string, unknown>>,
		step: WorkflowStep,
	): Promise<{ ok: true }> {
		return step.do("noop", async () => ({ ok: true as const }));
	}
}
