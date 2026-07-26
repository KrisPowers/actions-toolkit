import { Outlet, useParams } from "react-router-dom";
import { useRepo } from "../hooks/useRepos";
import PageHeader from "../components/common/PageHeader";
import SettingsSidebar from "../components/settings/SettingsSidebar";

export default function RepoSettingsLayout() {
  const { repoId } = useParams();
  const { data: repo } = useRepo(repoId);

  if (!repo) return null;

  return (
    <div>
      <PageHeader title={`${repo.owner}/${repo.name} settings`} />

      <div className="mt-6 grid grid-cols-1 gap-8 md:grid-cols-[240px_1fr]">
        <div className="md:sticky md:top-6 md:self-start">
          <SettingsSidebar repoId={repo.id} />
        </div>
        <div className="min-w-0">
          <Outlet />
        </div>
      </div>
    </div>
  );
}
