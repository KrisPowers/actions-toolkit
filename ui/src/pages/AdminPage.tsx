import { Navigate, Outlet } from "react-router-dom";
import { useMe } from "../hooks/useAuth";
import PageHeader from "../components/common/PageHeader";
import AdminSidebar from "../components/admin/AdminSidebar";

// Everything here is gated to the admin role on the backend too (see AdminUser in
// core/src/auth/middleware.rs), so a non-admin landing on /admin directly (rather than through
// the hidden nav link) is bounced straight back rather than shown a page full of 403s.
export default function AdminPage() {
  const { data: me, isLoading } = useMe();

  if (isLoading) return null;
  if (me?.role !== "admin") return <Navigate to="/" replace />;

  return (
    <div className="max-w-6xl">
      <PageHeader title="Admin" subtitle="Instance-wide access, login history, and tunnel visibility." />

      <div className="mt-5 grid grid-cols-1 gap-6 md:grid-cols-[200px_1fr]">
        <AdminSidebar />
        <div className="min-w-0">
          <Outlet />
        </div>
      </div>
    </div>
  );
}
