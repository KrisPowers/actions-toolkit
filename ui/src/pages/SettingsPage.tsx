import { Outlet } from "react-router-dom";
import PageHeader from "../components/common/PageHeader";
import InstanceSettingsSidebar from "../components/settings/InstanceSettingsSidebar";

export default function SettingsPage() {
  return (
    <div className="max-w-6xl">
      <PageHeader title="Settings" />

      <div className="mt-5 grid grid-cols-1 gap-6 md:grid-cols-[200px_1fr]">
        <InstanceSettingsSidebar />
        <div className="min-w-0">
          <Outlet />
        </div>
      </div>
    </div>
  );
}
