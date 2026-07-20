import { useParams } from "react-router-dom";
import { useRepo } from "../../hooks/useRepos";
import SecretsCard from "../../components/settings/SecretsCard";

export default function RepoSecretsSettingsPage() {
  const { repoId } = useParams();
  const { data: repo } = useRepo(repoId);

  if (!repo) return null;

  return <SecretsCard repoId={repo.id} />;
}
