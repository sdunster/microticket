export default function LoadingIndicator() {
  return (
    <div className="flex flex-col items-center justify-center gap-4 px-6 py-12">
      <div
        role="status"
        aria-label="Loading"
        className="size-10 animate-spin rounded-full border-4 border-line border-t-accent motion-reduce:animate-none"
      />
    </div>
  );
}
