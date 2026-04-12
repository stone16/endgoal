import { FreezeSession } from "@/features/freeze/freeze-session";

type FreezeSessionPageProps = {
  params: Promise<{
    id: string;
  }>;
};

export default async function FreezeSessionPage({
  params,
}: FreezeSessionPageProps) {
  const { id } = await params;

  return <FreezeSession nodeId={id} />;
}
