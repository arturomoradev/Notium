// Only /sync/* reaches this code (run_worker_first); everything else is a
// static file from dist-web. The request, Authorization header included, goes
// unchanged to the notium-sync Worker.
interface Env {
  SYNC: Fetcher;
}

export default {
  fetch(request: Request, env: Env): Promise<Response> {
    return env.SYNC.fetch(request);
  },
};
