import PageHeader from "../components/common/PageHeader";
import GithubConnectionCard from "../components/settings/GithubConnectionCard";
import RuntimeSettingsCard from "../components/settings/RuntimeSettingsCard";
import BucketSettingsCard from "../components/settings/BucketSettingsCard";

export default function SettingsPage() {
  return (
    <div className="max-w-6xl">
      <PageHeader title="Settings" />

      <div className="mt-5 grid grid-cols-1 gap-5 xl:grid-cols-2">
        <GithubConnectionCard />
        <RuntimeSettingsCard />
        <BucketSettingsCard />
      </div>
    </div>
  );
}
