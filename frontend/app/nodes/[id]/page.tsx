import { ObjectiveTree } from "@/features/nodes/objective-tree";

type NodeTreePageProps = {
  params: Promise<{
    id: string;
  }>;
  searchParams: Promise<{
    panel?: string | string[];
  }>;
};

export default async function NodeTreePage({
  params,
  searchParams,
}: NodeTreePageProps) {
  const { id } = await params;
  const query = await searchParams;
  const panel =
    typeof query.panel === "string" && query.panel.trim() ? query.panel : null;

  return <ObjectiveTree initialSelectedNodeId={panel} rootId={id} />;
}
