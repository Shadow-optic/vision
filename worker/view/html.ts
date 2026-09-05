/**
 * Auto-escaping HTML templates.
 *
 * Every value interpolated into `html` is escaped unless it is a `Fragment`
 * produced by `html` itself or by `raw()`. Official names, offices, and
 * finding summaries come from the backend, so escaping is a publication
 * guardrail and not just an XSS control: a card must render the record as
 * written, never as markup.
 */

const ENTITIES: Record<string, string> = {
	"&": "&amp;",
	"<": "&lt;",
	">": "&gt;",
	'"': "&quot;",
	"'": "&#39;",
};

export function escapeHtml(value: string): string {
	return value.replace(/[&<>"']/g, (c) => ENTITIES[c] as string);
}

export class Fragment {
	constructor(readonly value: string) {}
	toString(): string {
		return this.value;
	}
}

/** Marks already-safe markup. Only ever call this on literals we control. */
export function raw(value: string): Fragment {
	return new Fragment(value);
}

export type Renderable =
	| Fragment
	| string
	| number
	| boolean
	| null
	| undefined
	| Renderable[];

function render(value: Renderable): string {
	if (value === null || value === undefined || value === false) return "";
	if (value === true) return "";
	if (value instanceof Fragment) return value.value;
	if (Array.isArray(value)) return value.map(render).join("");
	return escapeHtml(String(value));
}

export function html(
	strings: TemplateStringsArray,
	...values: Renderable[]
): Fragment {
	let out = strings[0] ?? "";
	for (let i = 0; i < values.length; i++) {
		out += render(values[i]) + (strings[i + 1] ?? "");
	}
	return new Fragment(out);
}

/** Escapes a value for use inside a double-quoted attribute. */
export function attr(value: string): string {
	return escapeHtml(value);
}
