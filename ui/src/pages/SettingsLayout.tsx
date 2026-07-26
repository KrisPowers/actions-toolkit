import { Outlet } from "react-router-dom";
import PageHeader from "../components/common/PageHeader";
import AccountSettingsSidebar from "../components/settings/AccountSettingsSidebar";

export default function SettingsLayout() {
  return (
    <div>
      <PageHeader title="Settings" subtitle="Manage your GitHub connection and this instance's runtime configuration." />

      <div className="mt-6 grid grid-cols-1 gap-8 md:grid-cols-[240px_1fr]">
        <div className="md:sticky md:top-6 md:self-start">
          <AccountSettingsSidebar />
        </div>
        <div className="min-w-0">
          <Outlet />
        </div>
      </div>
    </div>
  );
}
