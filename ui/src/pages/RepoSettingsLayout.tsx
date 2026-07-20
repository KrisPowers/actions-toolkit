import { Outlet, useParams } from "react-router-dom";
import { useRepo } from "../hooks/useRepos";
import PageHeader from "../components/common/PageHeader";
import SettingsSidebar from "../components/settings/SettingsSidebar";

export default function RepoSettingsLayout() {
  const { repoId } = useParams();
  const { data: repo } = useRepo(repoId);

  if (!repo) return null;

  return (
    <div className="max-w-6xl">
      <PageHeader title={`${repo.owner}/${repo.name} settings`} />

      <div className="mt-5 grid grid-cols-1 gap-6 md:grid-cols-[200px_1fr]">
        <SettingsSidebar repoId={repo.id} />
        <div className="min-w-0">
          <Outlet />
        </div>
      </div>
    </div>
  );
}
