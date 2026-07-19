import { matchPath, useLocation } from "react-router-dom";

// Callers may be rendered as a sibling of <AppRoutes>'s <Routes>, not as a descendant of the
// matched Route, so useParams() has no route context there and always returns {}. matchPath
// against the raw location works from anywhere, matched-tree or not.
//
// The pattern requires a :section segment (workflows, runs, settings, ...) after the id, not
// just "/repos/:repoId/*", because the static route /repos/connect otherwise matches with
// repoId="connect" and shows the repo tab bar on the connect-a-repo page.
export function useRepoIdFromLocation(): string | undefined {
  const location = useLocation();
  const match = matchPath("/repos/:repoId/:section/*", location.pathname);
  return match?.params.repoId;
}
