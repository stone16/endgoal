export default function FreezeSessionLoading() {
  return (
    <main className="min-h-screen bg-stone-50">
      <div className="mx-auto flex w-full max-w-6xl flex-col gap-6 px-5 py-6 sm:px-8 lg:px-10">
        <div className="rounded-lg border border-stone-200 bg-white p-5">
          <div className="h-5 w-36 animate-pulse rounded bg-stone-100" />
          <div className="mt-3 h-9 max-w-2xl animate-pulse rounded bg-stone-100" />
          <div className="mt-5 grid gap-2 sm:grid-cols-4">
            {Array.from({ length: 4 }).map((_, index) => (
              <div
                key={index}
                className="h-10 animate-pulse rounded-md bg-stone-100"
              />
            ))}
          </div>
        </div>
        <div className="grid gap-6 lg:grid-cols-[minmax(0,1fr)_20rem]">
          <div className="h-[34rem] animate-pulse rounded-lg border border-stone-200 bg-white" />
          <div className="h-80 animate-pulse rounded-lg border border-stone-200 bg-white" />
        </div>
      </div>
    </main>
  );
}
