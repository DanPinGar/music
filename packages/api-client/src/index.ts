import createClient, { type Middleware } from "openapi-fetch";
import type { components, paths } from "./schema";

export type { components, paths };

export type Video = components["schemas"]["VideoDto"];
export type ImportVideoRequest = components["schemas"]["ImportVideoRequest"];
export type SystemInfo = components["schemas"]["SystemInfo"];
export type Problem = components["schemas"]["Problem"];

/**
 * Address of a video's media for `<video src>`. Media elements cannot send headers, so the
 * token travels as a query parameter (the API accepts both).
 */
export function mediaUrl({ baseUrl, token }: ApiClientOptions, videoId: string): string {
  return `${baseUrl}/videos/${encodeURIComponent(videoId)}/media?token=${encodeURIComponent(token)}`;
}

export interface ApiClientOptions {
  /** Address of the core API, e.g. http://127.0.0.1:52817 */
  readonly baseUrl: string;
  /** Session token sent as `Authorization: Bearer <token>`. */
  readonly token: string;
}

/** Typed client of the Segno core API; paths and payloads come from its OpenAPI document. */
export function createApiClient({ baseUrl, token }: ApiClientOptions) {
  const client = createClient<paths>({ baseUrl });
  const auth: Middleware = {
    onRequest({ request }) {
      request.headers.set("Authorization", `Bearer ${token}`);
      return request;
    },
  };
  client.use(auth);
  return client;
}

export type ApiClient = ReturnType<typeof createApiClient>;

/** A request that the API answered with an error status. */
export class ApiRequestError extends Error {
  readonly status: number;
  readonly problem: Problem | undefined;

  constructor(status: number, problem: Problem | undefined) {
    super(problem?.detail ?? problem?.title ?? `Request failed with status ${status}`);
    this.name = "ApiRequestError";
    this.status = status;
    this.problem = problem;
  }
}

/** The response data, or an {@link ApiRequestError} carrying the problem details. */
export function expectData<T>(result: { data?: T; error?: unknown; response: Response }): T {
  if (result.error === undefined && result.data !== undefined) return result.data;
  throw new ApiRequestError(result.response.status, asProblem(result.error));
}

function asProblem(error: unknown): Problem | undefined {
  if (typeof error === "object" && error !== null && "status" in error && "title" in error) {
    return error as Problem;
  }
  return undefined;
}
