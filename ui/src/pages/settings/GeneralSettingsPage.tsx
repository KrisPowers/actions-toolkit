import GithubConnectionCard from "../../components/settings/GithubConnectionCard";
import RuntimeSettingsCard from "../../components/settings/RuntimeSettingsCard";
import BucketSettingsCard from "../../components/settings/BucketSettingsCard";

export default function GeneralSettingsPage() {
  return (
    <div className="grid grid-cols-1 gap-5 xl:grid-cols-2">
      <GithubConnectionCard />
      <RuntimeSettingsCard />
      <BucketSettingsCard />
    </div>
  );
}
