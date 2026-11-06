import { Check, LoaderCircle, TriangleAlert } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import type { MessageKey, Translator } from "../../lib/i18n";
import { jobsContinue, jobsDismiss, jobsRetryFailed, jobsStop, type Job } from "../../lib/ipc";
import { isBackground, useJobs } from "../../stores/jobs";
import { addToast } from "../../stores/toasts";
import { Button, Progress } from "../ui";

const RUNNING: Record<Job["kind"]["type"], MessageKey> = {
  ordersVisibility: "market.bulkHiding",
  ordersDelete: "market.bulkDeleting",
  reprice: "jobs.running.reprice",
  workbench: "jobs.running.workbench",
  pricesLoad: "jobs.running.prices",
};
const NAME: Record<Job["kind"]["type"], MessageKey> = {
  ordersVisibility: "jobs.name.visibility",
  ordersDelete: "jobs.name.delete",
  reprice: "jobs.name.reprice",
  workbench: "jobs.name.workbench",
  pricesLoad: "jobs.name.prices",
};
const MAX_REASONS = 5;

const active = (j: Job) => j.state === "running" || j.state === "stopping";
const attention = (j: Job) => j.state === "interrupted" || j.failed > 0;
/** Items that went through, failed ones included: what "12 of 40" counts. */
const processed = (j: Job) => j.done + j.failed;

/** The one line a job gets: its progress while it runs, its outcome after. */
function summary(job: Job, tr: Translator): string {
  if (active(job)) {
    const key = job.kind.type === "ordersVisibility" && job.kind.visible ? "market.bulkShowing" : RUNNING[job.kind.type];
    return tr(key, { done: processed(job), total: job.total });
  }
  if (job.state === "interrupted") return tr("jobs.interrupted", { done: processed(job), total: job.total });
  if (job.failed > 0) return tr("jobs.doneFailed", { done: job.done, failed: job.failed });
  return tr("jobs.allDone", { count: job.done });
}

function JobRow({ job, tr }: { job: Job; tr: Translator }) {
  const act = (run: () => Promise<unknown>) => () => void run().catch((e: Error) => addToast({ level: "error", message: e.message }));
  const failed = job.items.filter((i) => i.status === "failed");
  const left = job.total - processed(job);
  // The workbench keeps its own review and retry flow; the price refresh has nothing to repeat.
  const repeatable = job.kind.type !== "workbench" && job.kind.type !== "pricesLoad";
  return (
    <li className="flex flex-col gap-1.5 border-t border-border py-2 first:border-t-0" data-job={job.kind.type} data-job-state={job.state}>
      <div className="flex items-center gap-2 text-sm">
        <span className="font-semibold">{tr(NAME[job.kind.type])}</span>
        <span className="ml-auto tabular-nums text-text-secondary">{active(job) ? tr("jobs.ofTotal", { done: processed(job), total: job.total }) : summary(job, tr)}</span>
      </div>
      {active(job) && <Progress label={tr(NAME[job.kind.type])} value={job.total ? (processed(job) / job.total) * 100 : 0} />}
      {active(job) && !isBackground(job) && (
        <Button size="sm" className="self-start" disabled={job.state === "stopping"} onClick={act(() => jobsStop(job.id))}>
          {tr(job.state === "stopping" ? "jobs.stopping" : "market.bulkStop")}
        </Button>
      )}
      {job.state === "interrupted" && <p className="m-0 text-xs text-text-muted">{tr("jobs.left", { count: left })}</p>}
      {failed.length > 0 && !active(job) && (
        <ul className="m-0 list-none p-0 text-xs text-danger">
          {failed.slice(0, MAX_REASONS).map((i) => (
            <li key={i.id} className="truncate" title={`${i.name}: ${i.reason ?? ""}`}>
              {tr("jobs.failedItem", { name: i.name, reason: i.reason ?? "" })}
            </li>
          ))}
          {failed.length > MAX_REASONS && <li>{tr("jobs.more", { count: failed.length - MAX_REASONS })}</li>}
        </ul>
      )}
      {!active(job) && attention(job) && (
        <div className="flex flex-wrap gap-2">
          {job.state === "interrupted" && repeatable && left > 0 && (
            <Button size="sm" variant="primary" onClick={act(() => jobsContinue(job.id))}>
              {tr("jobs.continue", { count: left })}
            </Button>
          )}
          {job.state === "done" && repeatable && failed.length > 0 && (
            <Button size="sm" variant="primary" onClick={act(() => jobsRetryFailed(job.id))}>
              {tr("jobs.retry", { count: failed.length })}
            </Button>
          )}
          <Button size="sm" variant="ghost" onClick={act(() => jobsDismiss(job.id))}>
            {tr("jobs.dismiss")}
          </Button>
        </div>
      )}
    </li>
  );
}

/** Titlebar pill for background jobs: visible while one runs or needs the user; opens the list. */
export default function JobsPill({ tr }: { tr: Translator }) {
  const { jobs } = useJobs();
  const [open, setOpen] = useState(false);
  const box = useRef<HTMLDivElement>(null);
  const user = jobs.filter((j) => !isBackground(j));
  // What the pill says: the user's running job first, then one that needs them, then the latest finish, and only
  // then a background job (muted).
  const lead = user.find(active) ?? user.find(attention) ?? user[user.length - 1] ?? jobs.find((j) => isBackground(j) && active(j));

  useEffect(() => {
    if (!open) return;
    const away = (e: MouseEvent) => box.current && !box.current.contains(e.target as Node) && setOpen(false);
    const esc = (e: KeyboardEvent) => e.key === "Escape" && setOpen(false);
    document.addEventListener("mousedown", away);
    document.addEventListener("keydown", esc);
    return () => {
      document.removeEventListener("mousedown", away);
      document.removeEventListener("keydown", esc);
    };
  }, [open]);

  if (!lead) return null;
  const text = summary(lead, tr);
  const quiet = isBackground(lead);
  const Icon = quiet ? null : active(lead) ? LoaderCircle : attention(lead) ? TriangleAlert : Check;
  return (
    <div ref={box} className="relative">
      <button type="button" className={`titlebar-pill ${quiet ? "text-text-muted" : ""}`} aria-label={tr("jobs.pillLabel", { text })} aria-expanded={open} aria-haspopup="dialog" data-jobs-pill={quiet ? "background" : lead.state} onClick={() => setOpen((o) => !o)}>
        {Icon && <Icon className={`size-3.5 ${active(lead) ? "animate-spin" : ""}`} aria-hidden />}
        <span className="titlebar-pill-text">{text}</span>
      </button>
      {open && (
        <div role="dialog" aria-label={tr("jobs.title")} className="fixed right-3 top-11 z-50 w-80 max-w-[calc(100vw-1.5rem)] rounded-lg border border-border bg-bg-raised p-3 shadow-lg">
          <h3 className="m-0 mb-1 text-sm font-semibold">{tr("jobs.title")}</h3>
          <ul className="m-0 list-none p-0">
            {jobs.map((j) => (
              <JobRow key={j.id} job={j} tr={tr} />
            ))}
          </ul>
        </div>
      )}
    </div>
  );
}
