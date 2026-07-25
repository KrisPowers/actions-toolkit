import { Box, Server } from "lucide-react";
import GithubMark from "../common/GithubMark";
import SettingsNav from "./SettingsNav";

export default function AccountSettingsSidebar() {
  return (
    <SettingsNav
      sections={[
        { to: "/settings/github", icon: GithubMark, label: "GitHub connection" },
        { to: "/settings/runtime", icon: Server, label: "Runtime" },
        { to: "/settings/bucket", icon: Box, label: "Bucket / sandbox" },
      ]}
    />
  );
}
