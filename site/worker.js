// atk.krispowers.dev: serves the install scripts from the repo's main branch.
//
//   curl -fsSL https://atk.krispowers.dev | sh      (macOS, Linux)
//   irm https://atk.krispowers.dev | iex            (Windows PowerShell)
//
// `/install.sh` and `/install.ps1` always return that script. `/` picks one from the
// client: PowerShell gets install.ps1, curl and wget get install.sh, browsers go to GitHub.

const RAW = "https://raw.githubusercontent.com/KrisPowers/actions-toolkit/main/";
const REPO = "https://github.com/KrisPowers/actions-toolkit";

function pick(path, agent) {
  if (path === "/install.sh" || path === "/install.ps1") return path.slice(1);
  if (path !== "/") return null;
  if (/PowerShell/i.test(agent)) return "install.ps1";
  if (/^(curl|wget)\//i.test(agent)) return "install.sh";
  return null;
}

export default {
  async fetch(request) {
    const url = new URL(request.url);
    const script = pick(url.pathname, request.headers.get("user-agent") || "");
    if (!script) return Response.redirect(REPO, 302);
    const upstream = await fetch(RAW + script, { cf: { cacheTtl: 300 } });
    if (!upstream.ok) return new Response("install script unavailable\n", { status: 502 });
    return new Response(upstream.body, {
      headers: { "content-type": "text/plain; charset=utf-8", "cache-control": "public, max-age=300" },
    });
  },
};
