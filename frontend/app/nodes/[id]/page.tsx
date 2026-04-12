import { ObjectiveTree } from "@/features/nodes/objective-tree";

type NodeTreePageProps = {
  params: Promise<{
    id: string;
  }>;
};

export default async function NodeTreePage({ params }: NodeTreePageProps) {
  const { id } = await params;

  return <ObjectiveTree rootId={id} />;
}
