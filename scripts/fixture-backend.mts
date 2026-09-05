/**
 * Local-only stand-in for the Rust `vi-api` service, for working on the public
 * site without Postgres. It serves the same fixtures the test suite uses.
 *
 *   npm run dev:fixtures                 # this server on :8788
 *   npx wrangler dev --var VI_API_ORIGIN:http://localhost:8788
 *
 * Never point a deployed Worker at this. It contains invented records.
 */
import { createServer } from "node:http";
import { handleBackendRequest } from "../test/backend-stub.ts";

const PORT = Number(process.env.PORT ?? 8788);

createServer(async (req, res) => {
	const request = new Request(`http://localhost:${PORT}${req.url ?? "/"}`, {
		method: req.method ?? "GET",
	});
	const response = handleBackendRequest(request as unknown as Parameters<typeof handleBackendRequest>[0]);
	res.statusCode = response.status;
	response.headers.forEach((value, key) => res.setHeader(key, value));
	res.end(await response.text());
}).listen(PORT, () => {
	console.log(`fixture vi-api listening on http://localhost:${PORT}`);
	console.log("records are fictional — local UI development only");
});
