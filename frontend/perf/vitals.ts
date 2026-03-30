/**
 * Web Vitals performance monitoring — #323
 *
 * Tracks LCP, FID/INP, CLS, FCP, TTFB and reports to console (dev)
 * or a configurable analytics endpoint (prod).
 *
 * Usage:
 *   import { initPerformanceMonitoring } from "./perf/vitals";
 *   initPerformanceMonitoring();          // call once at app entry point
 */

export type VitalMetric = {
  name: string;
  value: number;
  rating: "good" | "needs-improvement" | "poor";
  delta: number;
  id: string;
};

type ReportFn = (metric: VitalMetric) => void;

// Thresholds per https://web.dev/vitals/
export const VITAL_THRESHOLDS: Record<string, [number, number]> = {
  LCP:  [2500, 4000],
  FID:  [100,  300],
  INP:  [200,  500],
  CLS:  [0.1,  0.25],
  FCP:  [1800, 3000],
  TTFB: [800,  1800],
};

export function rateVital(name: string, value: number): VitalMetric["rating"] {
  const t = VITAL_THRESHOLDS[name];
  if (!t) return "good";
  if (value <= t[0]) return "good";
  if (value <= t[1]) return "needs-improvement";
  return "poor";
}

function defaultReporter(metric: VitalMetric) {
  const emoji = metric.rating === "good" ? "✅" : metric.rating === "needs-improvement" ? "⚠️" : "❌";
  console.log(`[Vitals] ${emoji} ${metric.name}: ${metric.value.toFixed(1)} (${metric.rating})`);
}

/**
 * Send to an analytics endpoint in production.
 * Replace the URL with your real endpoint.
 */
function sendToAnalytics(metric: VitalMetric) {
  const body = JSON.stringify(metric);
  if (navigator.sendBeacon) {
    navigator.sendBeacon("/api/vitals", body);
  } else {
    fetch("/api/vitals", { method: "POST", body, keepalive: true }).catch(() => {});
  }
}

/**
 * Observe a PerformanceObserver entry type and call back with a VitalMetric.
 */
function observe(
  type: string,
  name: string,
  getValue: (entries: PerformanceEntryList) => number | null,
  report: ReportFn
) {
  if (!("PerformanceObserver" in window)) return;
  try {
    const po = new PerformanceObserver((list) => {
      const value = getValue(list.getEntries());
      if (value === null) return;
      report({ name, value, rating: rateVital(name, value), delta: value, id: `${name}-${Date.now()}` });
    });
    po.observe({ type, buffered: true });
  } catch {
    // Entry type not supported in this browser — silently skip
  }
}

export function initPerformanceMonitoring(reporter?: ReportFn) {
  const report: ReportFn = (metric) => {
    defaultReporter(metric);
    if (process.env.NODE_ENV === "production") sendToAnalytics(metric);
    reporter?.(metric);
  };

  // LCP — Largest Contentful Paint
  observe("largest-contentful-paint", "LCP", (entries) => {
    const last = entries[entries.length - 1] as PerformanceEntry & { startTime: number };
    return last?.startTime ?? null;
  }, report);

  // FID — First Input Delay
  observe("first-input", "FID", (entries) => {
    const e = entries[0] as PerformanceEntry & { processingStart: number; startTime: number };
    return e ? e.processingStart - e.startTime : null;
  }, report);

  // INP — Interaction to Next Paint
  observe("event", "INP", (entries) => {
    const durations = entries
      .filter((e) => e.duration > 40)
      .map((e) => e.duration);
    return durations.length ? Math.max(...durations) : null;
  }, report);

  // CLS — Cumulative Layout Shift
  let clsValue = 0;
  observe("layout-shift", "CLS", (entries) => {
    for (const e of entries as Array<PerformanceEntry & { hadRecentInput: boolean; value: number }>) {
      if (!e.hadRecentInput) clsValue += e.value;
    }
    return clsValue;
  }, report);

  // FCP — First Contentful Paint
  observe("paint", "FCP", (entries) => {
    const fcp = entries.find((e) => e.name === "first-contentful-paint");
    return fcp?.startTime ?? null;
  }, report);

  // TTFB — Time to First Byte
  observe("navigation", "TTFB", (entries) => {
    const nav = entries[0] as PerformanceNavigationTiming | undefined;
    return nav ? nav.responseStart - nav.requestStart : null;
  }, report);
}
