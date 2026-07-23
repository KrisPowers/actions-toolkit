import { useParams } from "react-router-dom";
import PageHeader from "../components/common/PageHeader";
import BackendTopology from "../components/runs/BackendTopology";
import { useBucketTopology } from "../hooks/useRunstats";

export default function BucketDetailPage() {
  const { bucketId } = useParams();
  const { data: topology, isLoading } = useBucketTopology(bucketId);

  if (isLoading || !topology) {
    return <p className="text-sm text-neutral-500">Loading…</p>;
  }

  return (
    <div className="flex h-full flex-col">
      <div className="pb-3">
        <PageHeader
          title="Bucket backend"
          subtitle="Every shell (and its shards) this triggering event spawned, with live runtime insights."
          backTo={`/repos/${topology.bucket.bucket.repo_id}/runs`}
          backLabel="Runs"
        />
      </div>
      <div className="min-h-0 flex-1 overflow-y-auto">
        <BackendTopology bucket={topology.bucket} shells={topology.shells} samples={[]} />
      </div>
    </div>
  );
}
