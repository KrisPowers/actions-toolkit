import { AlertTriangle, Download, KeyRound, ShieldCheck } from "lucide-react";
import SettingsNav from "./SettingsNav";

export default function SettingsSidebar({ repoId }: { repoId: string }) {
  return (
    <SettingsNav
      sections={[
        { to: `/repos/${repoId}/settings/secrets`, icon: KeyRound, label: "Secrets" },
        { to: `/repos/${repoId}/settings/access`, icon: ShieldCheck, label: "Access" },
        { to: `/repos/${repoId}/settings/data`, icon: Download, label: "Data" },
        { to: `/repos/${repoId}/settings/danger`, icon: AlertTriangle, label: "Danger zone", danger: true },
      ]}
    />
  );
}
