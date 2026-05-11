import { useState, useCallback } from 'react'

interface TacticSpec {
  product_name: string
  subproduct_name: string
  table_name: string
  file_name: string
  headers: string[]
  is_required: boolean
  sort_order: number
}

interface Detection {
  filename: string
  matched: boolean
  spec: TacticSpec | null
  header_match_pct: number
  missing_headers: string[]
  extra_headers: string[]
}

interface Timing {
  registry_match_ms: number
  cube_compile_ms: number
  cube_populate_ms: number
  narrative_eval_ms: number
  serialize_ms: number
}

interface CellEntry {
  category: string
  value: number
}

interface IngestedCube {
  label: string
  product: string
  subproduct: string
  table_name: string
  source_file: string
  dimension_count: number
  measure_count: number
  cells_written: number
  row_count: number
  values: Record<string, CellEntry[]>
}

interface NarrativeOutput {
  id: string
  severity: 'info' | 'success' | 'warning' | 'critical'
  text: string
  template_id: string
  evidence: Record<string, number>
}

interface TacticGroup {
  product: string
  subproduct: string
  csv_count: number
  cubes: IngestedCube[]
  narratives: NarrativeOutput[]
}

interface WorkspaceSummary {
  advertiser: string
  tactic_count: number
  total_csvs: number
  total_cells: number
  total_narratives: number
  summary_narratives: NarrativeOutput[]
}

interface ReviewCandidate {
  product_name: string
  subproduct_name: string
  table_name: string
  confidence: number
  source: string
}

interface ReviewItem {
  slide_index: number
  table_index: number
  slide_title: string | null
  table_title: string | null
  headers: string[]
  row_count: number
  first_row: string[]
  status: string
  best_guess: ReviewCandidate | null
  alternatives: ReviewCandidate[]
}

interface PptxMatchSummary {
  total_tables: number
  auto_resolved: number
  skipped: number
  duplicates: number
  review_needed: number
  unmatched: number
  review_items: ReviewItem[]
}

interface UploadResponse {
  schema_version: string
  processing_time_ms: number
  timing: Timing
  csv_count: number
  tactic_count: number
  label: string
  detections: Detection[]
  tactics: TacticGroup[]
  summary: WorkspaceSummary
  pptx_match_summary?: PptxMatchSummary
}

type ViewState = 'upload' | 'loading' | 'results'

function App() {
  const [view, setView] = useState<ViewState>('upload')
  const [response, setResponse] = useState<UploadResponse | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [dragOver, setDragOver] = useState(false)

  const handleUpload = useCallback(async (file: File) => {
    setView('loading')
    setError(null)
    try {
      const formData = new FormData()
      formData.append('file', file)
      const res = await fetch('/api/upload', {
        method: 'POST',
        body: formData,
      })
      if (!res.ok) {
        const text = await res.text()
        throw new Error(text || `HTTP ${res.status}`)
      }
      const data: UploadResponse = await res.json()
      setResponse(data)
      setView('results')
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Upload failed')
      setView('upload')
    }
  }, [])

  const onDrop = useCallback((e: React.DragEvent) => {
    e.preventDefault()
    setDragOver(false)
    const file = e.dataTransfer.files[0]
    if (file) handleUpload(file)
  }, [handleUpload])

  const onFileSelect = useCallback((e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0]
    if (file) handleUpload(file)
  }, [handleUpload])

  return (
    <div className="min-h-screen bg-[#f8f8f6] text-[#1a1a1a]">
      <header className="border-b border-neutral-200 px-6 py-4">
        <div className="max-w-5xl mx-auto flex items-center justify-between">
          <div className="flex items-center gap-3">
            <h1 className="text-xl font-bold tracking-tight">Mosaic</h1>
            <span className="text-xs px-2 py-0.5 bg-neutral-100 text-neutral-500 rounded-full border border-neutral-300">
              Large Numbers Model
            </span>
          </div>
          {response && (
            <span className="text-sm text-neutral-900 font-mono">
              Processed in {response.processing_time_ms.toFixed(1)}ms
            </span>
          )}
        </div>
      </header>

      <main className="max-w-5xl mx-auto px-6 py-8">
        {view === 'upload' && (
          <UploadView
            dragOver={dragOver}
            setDragOver={setDragOver}
            onDrop={onDrop}
            onFileSelect={onFileSelect}
            error={error}
          />
        )}
        {view === 'loading' && (
          <div className="flex flex-col items-center justify-center py-20">
            <div className="w-8 h-8 border-2 border-neutral-400 border-t-transparent rounded-full animate-spin mb-4" />
            <p className="text-neutral-500">Processing upload...</p>
          </div>
        )}
        {view === 'results' && response && (
          <ResultsView
            response={response}
            onReset={() => { setView('upload'); setResponse(null) }}
          />
        )}
      </main>
    </div>
  )
}

function UploadView({
  dragOver, setDragOver, onDrop, onFileSelect, error
}: {
  dragOver: boolean
  setDragOver: (v: boolean) => void
  onDrop: (e: React.DragEvent) => void
  onFileSelect: (e: React.ChangeEvent<HTMLInputElement>) => void
  error: string | null
}) {
  return (
    <div className="flex flex-col items-center">
      <h2 className="text-3xl font-bold mb-2">Marketing Report Demo</h2>
      <p className="text-neutral-500 mb-8 max-w-lg text-center">
        Upload a zip of marketing CSV exports. Mosaic auto-detects tactics from a
        190-entry registry, populates cubes, and generates narrative reports — instantly.
      </p>
      <div
        className={`w-full max-w-xl border-2 border-dashed rounded-xl p-12 text-center cursor-pointer transition-all ${
          dragOver
            ? 'border-neutral-900 bg-neutral-100'
            : 'border-neutral-300 hover:border-neutral-400 hover:bg-neutral-50'
        }`}
        onDragOver={(e) => { e.preventDefault(); setDragOver(true) }}
        onDragLeave={() => setDragOver(false)}
        onDrop={onDrop}
        onClick={() => document.getElementById('file-input')?.click()}
      >
        <div className="text-4xl mb-3">&#128193;</div>
        <p className="text-lg mb-1">Drop a zip or pptx file here</p>
        <p className="text-sm text-neutral-400">or click to browse</p>
        <input
          id="file-input"
          type="file"
          accept=".zip,.pptx,application/vnd.openxmlformats-officedocument.presentationml.presentation,application/zip"
          className="hidden"
          onChange={onFileSelect}
        />
      </div>
      {error && (
        <div className="mt-4 px-4 py-3 bg-red-50 border border-red-200 rounded-lg text-red-700 text-sm">
          {error}
        </div>
      )}
      <div className="mt-8 text-sm text-neutral-400">
        No LLM. No hallucination. Deterministic processing.
      </div>
    </div>
  )
}

function ResultsView({
  response, onReset
}: {
  response: UploadResponse
  onReset: () => void
}) {
  const [showPayload, setShowPayload] = useState(false)
  const [copied, setCopied] = useState(false)
  const unmatched = response.detections.filter(d => !d.matched)

  const copyAsMarkdown = useCallback(() => {
    const lines: string[] = []
    lines.push(`# ${response.label}`)
    lines.push('')
    lines.push(`*${response.csv_count} CSVs | ${response.tactic_count} tactic(s) | Processed in ${response.processing_time_ms.toFixed(1)}ms*`)
    lines.push('')

    // Summary
    if (response.summary.summary_narratives.length > 0) {
      lines.push('## Summary')
      lines.push('')
      for (const n of response.summary.summary_narratives) {
        const prefix = n.severity === 'critical' ? '> **ALERT:** ' : n.severity === 'warning' ? '> **Warning:** ' : '- '
        lines.push(`${prefix}${n.text}`)
      }
      lines.push('')
    }

    // Per-tactic
    for (const t of response.tactics) {
      lines.push(`## ${t.subproduct}`)
      lines.push('')
      lines.push(`*${t.product} | ${t.csv_count} CSVs | ${t.cubes.length} cubes*`)
      lines.push('')
      for (const n of t.narratives) {
        const prefix = n.severity === 'critical' ? '> **ALERT:** ' : n.severity === 'warning' ? '> **Warning:** ' : '- '
        lines.push(`${prefix}${n.text}`)
      }
      lines.push('')
    }

    lines.push('---')
    lines.push(`*Generated by Mosaic LNM v0.1.0 | ${new Date().toLocaleDateString()}*`)

    navigator.clipboard.writeText(lines.join('\n')).then(() => {
      setCopied(true)
      setTimeout(() => setCopied(false), 2000)
    })
  }, [response])

  return (
    <div>
      {/* Header bar */}
      <div className="flex items-center justify-between mb-6">
        <div>
          <h2 className="text-2xl font-bold">{response.label}</h2>
          <p className="text-neutral-500 text-sm">
            {response.csv_count} CSVs &middot; {response.tactic_count} tactic{response.tactic_count !== 1 ? 's' : ''} &middot; {response.summary.total_cells} cells &middot; {response.summary.total_narratives} insights
          </p>
        </div>
        <div className="flex gap-2">
          <button
            onClick={copyAsMarkdown}
            className="px-4 py-2 text-sm rounded-lg border border-neutral-300 text-neutral-600 hover:bg-neutral-100 transition-colors"
          >
            {copied ? 'Copied!' : 'Copy as Markdown'}
          </button>
          <button
            onClick={() => setShowPayload(!showPayload)}
            className="px-4 py-2 text-sm rounded-lg border border-neutral-300 text-neutral-600 hover:bg-neutral-100 transition-colors"
          >
            {showPayload ? 'Hide' : 'Show'} Payload
          </button>
          <button
            onClick={onReset}
            className="px-4 py-2 text-sm rounded-lg bg-neutral-900 text-white hover:bg-neutral-700 transition-colors"
          >
            Upload Another
          </button>
        </div>
      </div>

      {/* Timing badge */}
      <div className="mb-6 p-4 bg-neutral-50 border border-neutral-200 rounded-lg">
        <div className="flex items-center gap-4 text-sm font-mono flex-wrap">
          <span className="text-neutral-900 font-bold">
            Done {response.processing_time_ms.toFixed(1)}ms
          </span>
          <span className="text-neutral-300">|</span>
          <span className="text-neutral-500">registry {response.timing.registry_match_ms.toFixed(1)}ms</span>
          <span className="text-neutral-500">compile {response.timing.cube_compile_ms.toFixed(1)}ms</span>
          <span className="text-neutral-500">populate {response.timing.cube_populate_ms.toFixed(1)}ms</span>
          <span className="text-neutral-500">narrative {response.timing.narrative_eval_ms.toFixed(1)}ms</span>
          <span className="text-neutral-500">serialize {response.timing.serialize_ms.toFixed(1)}ms</span>
        </div>
      </div>

      {/* PPTX match summary banner */}
      {response.pptx_match_summary && (
        <PptxMatchBanner summary={response.pptx_match_summary} />
      )}

      {/* PPTX review panel */}
      {response.pptx_match_summary && response.pptx_match_summary.review_needed > 0 && (
        <ReviewPanel summary={response.pptx_match_summary} />
      )}

      {/* Summary narratives — cross-tactic overview */}
      {response.summary.summary_narratives.length > 0 && (
        <div className="mb-6">
          <h3 className="text-lg font-semibold mb-3">Summary</h3>
          <ul className="space-y-1">
            {response.summary.summary_narratives.map((n, i) => (
              <NarrativeCard key={i} narrative={n} />
            ))}
          </ul>
        </div>
      )}

      {/* Per-tactic sections */}
      {response.tactics.map((tactic, ti) => (
        <TacticSection key={ti} tactic={tactic} />
      ))}

      {/* Unmatched files — collapsed dropdown */}
      {unmatched.length > 0 && (
        <UnmatchedDropdown files={unmatched} />
      )}

      {/* ROI Calculator */}
      <ROICalculator processingTimeMs={response.processing_time_ms} />

      {/* Payload view */}
      {showPayload && (
        <div className="mt-6">
          <h3 className="text-lg font-semibold mb-3">Raw JSON Payload</h3>
          <pre className="p-4 bg-white border border-neutral-200 rounded-lg text-xs font-mono text-neutral-700 overflow-auto max-h-96">
            {JSON.stringify(response, null, 2)}
          </pre>
        </div>
      )}
    </div>
  )
}

// Real token costs from ignite-report-ai workers/src analysis.
// Input = system prompts + CSV data + platform context + benchmarks + pacing.
// Output = 7 parallel section agents generating narrative JSON.
const LLM = {
  input_tokens: 12000,            // context: prompts, CSV data, platform knowledge, benchmarks, pacing
  output_tokens: 5500,            // report: 7 section agents generating narrative sections
  input_cost_per_1k: 0.003,       // Claude Sonnet input pricing
  output_cost_per_1k: 0.015,      // Claude Sonnet output pricing
  seconds_per_report: 45,         // realistic for multi-section report with retries
}

function UnmatchedDropdown({ files }: { files: Detection[] }) {
  const [open, setOpen] = useState(false)
  return (
    <div className="mb-6">
      <button
        onClick={() => setOpen(!open)}
        className="flex items-center gap-2 text-sm text-neutral-500 hover:text-neutral-700 transition-colors"
      >
        <span className={`transition-transform ${open ? 'rotate-90' : ''}`}>&#9654;</span>
        <span>{files.length} unmatched file{files.length !== 1 ? 's' : ''} not in registry</span>
      </button>
      {open && (
        <div className="mt-2 pl-5 text-xs text-neutral-400 space-y-0.5">
          {files.map((d, i) => (
            <div key={i}>{d.filename}</div>
          ))}
        </div>
      )}
    </div>
  )
}

function ROICalculator({ processingTimeMs }: { processingTimeMs: number }) {
  const [reports, setReports] = useState(250)

  const totalTokens = LLM.input_tokens + LLM.output_tokens
  const costPerReport = (LLM.input_tokens / 1000) * LLM.input_cost_per_1k
    + (LLM.output_tokens / 1000) * LLM.output_cost_per_1k
  const llmTokensMonth = totalTokens * reports
  const llmCostMonth = costPerReport * reports
  const llmTimeMonthHours = (LLM.seconds_per_report * reports) / 3600
  const llmCostYear = llmCostMonth * 12
  const llmTimeYearHours = llmTimeMonthHours * 12

  const mosaicTimeSec = (processingTimeMs * reports) / 1000
  const mosaicTimeYearMin = (mosaicTimeSec * 12) / 60

  return (
    <div className="mb-8 border border-neutral-200 rounded-lg overflow-hidden">
      <div className="px-5 py-4 border-b border-neutral-200 bg-neutral-50">
        <h3 className="text-lg font-semibold">LLM vs Mosaic LNM &mdash; Cost &amp; Speed Comparison</h3>
        <div className="mt-3 flex items-center gap-3">
          <label className="text-sm text-neutral-500">Reports per month:</label>
          <input
            type="number"
            value={reports}
            onChange={e => setReports(Math.max(1, parseInt(e.target.value) || 1))}
            className="w-24 px-3 py-1.5 text-sm border border-neutral-300 rounded-lg text-center font-mono bg-white"
          />
        </div>
      </div>

      <div className="grid grid-cols-2 divide-x divide-neutral-200">
        {/* LLM column */}
        <div className="p-5">
          <h4 className="font-semibold text-neutral-500 mb-4 text-sm uppercase tracking-wide">LLM (Claude Sonnet)</h4>

          <div className="space-y-4 text-sm">
            <div>
              <div className="text-neutral-400 text-xs uppercase tracking-wide mb-1">Per report</div>
              <div className="text-red-700 font-mono">~{(LLM.input_tokens / 1000).toFixed(0)}K input tokens <span className="text-neutral-400">(context)</span></div>
              <div className="text-neutral-400 text-xs ml-4 font-mono">system prompts, CSV data, platform</div>
              <div className="text-neutral-400 text-xs ml-4 font-mono mb-1">knowledge, benchmarks, pacing</div>
              <div className="text-red-700 font-mono">~{(LLM.output_tokens / 1000).toFixed(1)}K output tokens <span className="text-neutral-400">(report)</span></div>
              <div className="text-neutral-400 text-xs ml-4 font-mono mb-1">7 section agents, narrative JSON</div>
              <div className="text-red-700 font-mono font-bold">${costPerReport.toFixed(3)}/report</div>
              <div className="text-red-700 font-mono">~{LLM.seconds_per_report}s processing</div>
            </div>

            <div>
              <div className="text-neutral-400 text-xs uppercase tracking-wide mb-1">{reports} reports / month</div>
              <div className="text-red-700 font-mono">{(llmTokensMonth / 1_000_000).toFixed(1)}M tokens</div>
              <div className="text-red-700 font-mono">${llmCostMonth.toFixed(2)}/month</div>
              <div className="text-red-700 font-mono">{llmTimeMonthHours.toFixed(1)} hours waiting</div>
            </div>

            <div>
              <div className="text-neutral-400 text-xs uppercase tracking-wide mb-1">Annual</div>
              <div className="text-red-700 font-mono font-bold">${llmCostYear.toFixed(0)}/year</div>
              <div className="text-red-700 font-mono font-bold">{llmTimeYearHours.toFixed(1)} hours/year</div>
            </div>
          </div>
        </div>

        {/* Mosaic column */}
        <div className="p-5">
          <h4 className="font-semibold text-green-700 mb-4 text-sm uppercase tracking-wide">Mosaic LNM</h4>

          <div className="space-y-4 text-sm">
            <div>
              <div className="text-neutral-400 text-xs uppercase tracking-wide mb-1">Per report</div>
              <div className="text-green-700 font-mono">0 input tokens <span className="text-neutral-400">(no context needed)</span></div>
              <div className="text-neutral-400 text-xs ml-4 font-mono mb-1">formula engine has the knowledge built in</div>
              <div className="text-green-700 font-mono">0 output tokens <span className="text-neutral-400">(deterministic)</span></div>
              <div className="text-neutral-400 text-xs ml-4 font-mono mb-1">templates, not generation</div>
              <div className="text-green-700 font-mono font-bold">$0.00/report</div>
              <div className="text-green-700 font-mono">{processingTimeMs.toFixed(1)}ms processing</div>
            </div>

            <div>
              <div className="text-neutral-400 text-xs uppercase tracking-wide mb-1">{reports} reports / month</div>
              <div className="text-green-700 font-mono">0 tokens</div>
              <div className="text-green-700 font-mono">$0.00/month</div>
              <div className="text-green-700 font-mono">{mosaicTimeSec.toFixed(1)} seconds total</div>
            </div>

            <div>
              <div className="text-neutral-400 text-xs uppercase tracking-wide mb-1">Annual</div>
              <div className="text-green-700 font-mono font-bold">$0/year</div>
              <div className="text-green-700 font-mono font-bold">{mosaicTimeYearMin.toFixed(1)} minutes/year</div>
            </div>
          </div>
        </div>
      </div>

      {/* Savings banner */}
      <div className="px-5 py-4 bg-green-50 border-t border-green-200">
        <div className="text-sm font-semibold text-green-800 mb-1">Savings with Mosaic LNM</div>
        <div className="flex flex-wrap gap-x-6 gap-y-1 text-sm text-green-700">
          <span className="font-mono font-bold">${llmCostYear.toFixed(0)}/year in token costs</span>
          <span className="font-mono font-bold">{llmTimeYearHours.toFixed(1)} hours/year in processing time</span>
          <span className="font-mono font-bold">{((totalTokens * reports * 12) / 1_000_000).toFixed(1)}M tokens/year eliminated</span>
        </div>
        <div className="mt-2 text-xs text-green-600">
          Zero hallucination risk &middot; No context window needed &middot; Deterministic &middot; Auditable
        </div>
      </div>
    </div>
  )
}

function PptxMatchBanner({ summary }: { summary: PptxMatchSummary }) {
  return (
    <div className="mb-6 p-4 bg-neutral-50 border border-neutral-200 rounded-lg">
      <div className="flex items-center gap-3 text-sm flex-wrap">
        <span className="font-semibold text-neutral-900">PPTX: {summary.total_tables} tables</span>
        <span className="text-neutral-300">—</span>
        <span className="text-green-700 font-medium">{summary.auto_resolved} matched</span>
        {summary.skipped > 0 && <span className="text-neutral-400">{summary.skipped} skipped</span>}
        {summary.duplicates > 0 && <span className="text-neutral-400">{summary.duplicates} duplicates</span>}
        {summary.review_needed > 0 && <span className="text-amber-600 font-medium">{summary.review_needed} need review</span>}
        {summary.unmatched > 0 && <span className="text-neutral-500">{summary.unmatched} unmatched</span>}
      </div>
    </div>
  )
}

type ReviewDecisionMap = Record<string, { action: 'confirm' | 'skip'; candidate?: ReviewCandidate }>

function ReviewPanel({ summary }: { summary: PptxMatchSummary }) {
  const [decisions, setDecisions] = useState<ReviewDecisionMap>({})
  const [saving, setSaving] = useState(false)
  const [saveResult, setSaveResult] = useState<string | null>(null)

  const itemKey = (item: ReviewItem) => `${item.slide_index}-${item.table_index}`

  const setConfirm = (item: ReviewItem, candidate: ReviewCandidate) => {
    setDecisions(prev => ({
      ...prev,
      [itemKey(item)]: { action: 'confirm', candidate },
    }))
  }

  const setSkip = (item: ReviewItem) => {
    setDecisions(prev => ({
      ...prev,
      [itemKey(item)]: { action: 'skip' },
    }))
  }

  const handleSave = async () => {
    const payload = summary.review_items
      .filter(item => decisions[itemKey(item)])
      .map(item => {
        const dec = decisions[itemKey(item)]
        if (dec.action === 'confirm' && dec.candidate) {
          return {
            slide_index: item.slide_index,
            table_index: item.table_index,
            action: 'confirm',
            product_name: dec.candidate.product_name,
            subproduct_name: dec.candidate.subproduct_name,
            table_name: dec.candidate.table_name,
          }
        }
        return {
          slide_index: item.slide_index,
          table_index: item.table_index,
          action: 'skip',
          reason: 'User skipped',
        }
      })

    if (payload.length === 0) return

    setSaving(true)
    setSaveResult(null)
    try {
      const res = await fetch('/api/pptx-review', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(payload),
      })
      if (!res.ok) {
        const text = await res.text()
        throw new Error(text || `HTTP ${res.status}`)
      }
      const data = await res.json()
      setSaveResult(`Saved ${data.saved} decision(s) to profile. Re-upload to see updated results.`)
    } catch (e) {
      setSaveResult(`Error: ${e instanceof Error ? e.message : 'Save failed'}`)
    } finally {
      setSaving(false)
    }
  }

  const decidedCount = Object.keys(decisions).length

  return (
    <div className="mb-6 border border-amber-200 rounded-lg overflow-hidden">
      <div className="px-5 py-4 bg-amber-50 border-b border-amber-200">
        <h3 className="text-lg font-semibold text-amber-800">
          PPTX Review — {summary.review_needed} table{summary.review_needed !== 1 ? 's' : ''} need confirmation
        </h3>
        <p className="text-sm text-amber-600 mt-1">
          Confirm a mapping or skip each table. Save decisions to the profile so future uploads resolve automatically.
        </p>
      </div>

      <div className="divide-y divide-neutral-200">
        {summary.review_items.map((item) => {
          const key = itemKey(item)
          const dec = decisions[key]
          const allCandidates = [
            ...(item.best_guess ? [item.best_guess] : []),
            ...item.alternatives.filter(a =>
              !item.best_guess || a.product_name !== item.best_guess.product_name
              || a.subproduct_name !== item.best_guess.subproduct_name
              || a.table_name !== item.best_guess.table_name
            ),
          ].slice(0, 3)

          return (
            <div key={key} className={`px-5 py-4 ${dec?.action === 'skip' ? 'bg-neutral-50 opacity-60' : 'bg-white'}`}>
              <div className="flex items-start justify-between mb-2">
                <div>
                  <span className="font-medium text-sm">
                    Slide {item.slide_index}, Table {item.table_index}
                  </span>
                  {item.table_title && (
                    <span className="text-neutral-500 text-sm ml-2">— {item.table_title}</span>
                  )}
                  <span className="ml-2 text-xs px-1.5 py-0.5 rounded bg-amber-100 text-amber-700">
                    {item.status}
                  </span>
                </div>
              </div>

              <div className="text-xs text-neutral-500 mb-2">
                <span className="font-mono">{item.headers.slice(0, 6).join(', ')}{item.headers.length > 6 ? ', ...' : ''}</span>
                <span className="ml-2">{item.row_count} row{item.row_count !== 1 ? 's' : ''}</span>
              </div>
              {item.first_row.length > 0 && (
                <div className="text-xs text-neutral-400 font-mono mb-3 truncate">
                  Row 1: {item.first_row.slice(0, 6).join(', ')}{item.first_row.length > 6 ? ', ...' : ''}
                </div>
              )}

              <div className="flex items-center gap-2 flex-wrap">
                {allCandidates.length > 0 ? (
                  <select
                    className="text-sm border border-neutral-300 rounded-lg px-3 py-1.5 bg-white min-w-[280px]"
                    value={dec?.action === 'confirm' && dec.candidate
                      ? `${dec.candidate.product_name}/${dec.candidate.subproduct_name}/${dec.candidate.table_name}`
                      : ''}
                    onChange={(e) => {
                      const val = e.target.value
                      if (!val) return
                      const match = allCandidates.find(c =>
                        `${c.product_name}/${c.subproduct_name}/${c.table_name}` === val
                      )
                      if (match) setConfirm(item, match)
                    }}
                  >
                    <option value="">Select mapping...</option>
                    {allCandidates.map((c, ci) => (
                      <option
                        key={ci}
                        value={`${c.product_name}/${c.subproduct_name}/${c.table_name}`}
                      >
                        {c.product_name} / {c.subproduct_name} / {c.table_name} ({(c.confidence * 100).toFixed(0)}%)
                      </option>
                    ))}
                  </select>
                ) : (
                  <span className="text-sm text-neutral-400">No candidates found</span>
                )}
                <button
                  onClick={() => setSkip(item)}
                  className={`text-sm px-3 py-1.5 rounded-lg border transition-colors ${
                    dec?.action === 'skip'
                      ? 'bg-neutral-200 border-neutral-300 text-neutral-600'
                      : 'border-neutral-300 text-neutral-500 hover:bg-neutral-100'
                  }`}
                >
                  {dec?.action === 'skip' ? 'Skipped' : 'Skip'}
                </button>
                {dec?.action === 'confirm' && dec.candidate && (
                  <span className="text-xs text-green-600">
                    Confirmed: {dec.candidate.subproduct_name} / {dec.candidate.table_name}
                  </span>
                )}
              </div>
            </div>
          )
        })}
      </div>

      <div className="px-5 py-4 bg-neutral-50 border-t border-neutral-200 flex items-center justify-between">
        <span className="text-sm text-neutral-500">
          {decidedCount} of {summary.review_items.length} decided
        </span>
        <div className="flex items-center gap-3">
          {saveResult && (
            <span className={`text-sm ${saveResult.startsWith('Error') ? 'text-red-600' : 'text-green-600'}`}>
              {saveResult}
            </span>
          )}
          <button
            onClick={handleSave}
            disabled={saving || decidedCount === 0}
            className={`px-4 py-2 text-sm rounded-lg transition-colors ${
              saving || decidedCount === 0
                ? 'bg-neutral-200 text-neutral-400 cursor-not-allowed'
                : 'bg-neutral-900 text-white hover:bg-neutral-700'
            }`}
          >
            {saving ? 'Saving...' : 'Save All Decisions'}
          </button>
        </div>
      </div>
    </div>
  )
}

function TacticSection({ tactic }: { tactic: TacticGroup }) {
  const [showCubes, setShowCubes] = useState(false)

  return (
    <div className="mb-8">
      <div className="flex items-center justify-between mb-3">
        <div>
          <h3 className="text-lg font-semibold">
            {tactic.subproduct}
          </h3>
          <p className="text-neutral-500 text-xs">
            {tactic.product} &middot; {tactic.csv_count} CSV{tactic.csv_count !== 1 ? 's' : ''} &middot; {tactic.cubes.length} cube{tactic.cubes.length !== 1 ? 's' : ''}
          </p>
        </div>
        <button
          onClick={() => setShowCubes(!showCubes)}
          className="text-xs px-3 py-1 rounded border border-neutral-300 text-neutral-500 hover:bg-neutral-100 transition-colors"
        >
          {showCubes ? 'Hide' : 'Show'} Data
        </button>
      </div>

      {/* Per-tactic narratives */}
      {tactic.narratives.length > 0 && (
        <ul className="space-y-1 mb-4">
          {tactic.narratives.map((n, i) => (
            <NarrativeCard key={i} narrative={n} />
          ))}
        </ul>
      )}

      {/* Expandable cube data */}
      {showCubes && tactic.cubes.length > 0 && (
        <div className="space-y-3">
          {tactic.cubes.map((cube, i) => (
            <CubeCard key={i} cube={cube} />
          ))}
        </div>
      )}
    </div>
  )
}

/** Render **bold** markers in narrative text as <strong> tags. */
function renderNarrativeText(text: string) {
  const parts = text.split(/\*\*(.*?)\*\*/g)
  return parts.map((part, i) =>
    i % 2 === 1
      ? <strong key={i} className="font-semibold text-[#111]">{part}</strong>
      : <span key={i}>{part}</span>
  )
}

function NarrativeCard({ narrative }: { narrative: NarrativeOutput }) {
  const dotColors: Record<string, string> = {
    info: 'bg-neutral-300',
    success: 'bg-emerald-400',
    warning: 'bg-amber-400',
    critical: 'bg-red-500',
  }
  const borderColors: Record<string, string> = {
    info: '',
    success: 'border-l-2 border-l-emerald-400',
    warning: 'border-l-2 border-l-amber-400',
    critical: 'border-l-2 border-l-red-400',
  }

  return (
    <li className={`flex items-start gap-2.5 py-1 pl-1 ${borderColors[narrative.severity] || ''}`}>
      <span className={`mt-[7px] h-1.5 w-1.5 rounded-full flex-shrink-0 ${dotColors[narrative.severity] || dotColors.info}`} />
      <p className="text-sm text-[#2a2a2a] leading-relaxed">
        {renderNarrativeText(narrative.text)}
      </p>
    </li>
  )
}

function CubeCard({ cube }: { cube: IngestedCube }) {
  const [expanded, setExpanded] = useState(false)
  const measureNames = Object.keys(cube.values)

  return (
    <div className="bg-white border border-neutral-200 rounded-lg overflow-hidden">
      <div
        className="flex items-center justify-between p-3 cursor-pointer hover:bg-neutral-50 transition-colors"
        onClick={() => setExpanded(!expanded)}
      >
        <div>
          <span className="font-medium text-sm">{cube.table_name}</span>
          <span className="text-neutral-400 text-xs ml-2">
            {cube.cells_written} cells &middot; {cube.measure_count} measures &middot; {cube.row_count} rows
          </span>
        </div>
        <span className="text-neutral-400 text-sm">{expanded ? '\u2212' : '+'}</span>
      </div>
      {expanded && (
        <div className="border-t border-neutral-200 p-3">
          <div className="overflow-x-auto">
            <table className="w-full text-sm">
              <thead>
                <tr className="border-b border-neutral-100">
                  <th className="text-left py-1.5 pr-4 font-medium text-neutral-500 text-xs uppercase tracking-wide">
                    Category
                  </th>
                  {measureNames.map(m => (
                    <th key={m} className="text-right py-1.5 px-2 font-medium text-neutral-500 text-xs uppercase tracking-wide">
                      {m}
                    </th>
                  ))}
                </tr>
              </thead>
              <tbody>
                {cube.values[measureNames[0]]?.map((entry, rowIdx) => (
                  <tr key={rowIdx} className="border-b border-neutral-50">
                    <td className="py-1.5 pr-4 text-neutral-700">{entry.category}</td>
                    {measureNames.map(m => {
                      const val = cube.values[m]?.[rowIdx]?.value
                      return (
                        <td key={m} className="text-right py-1.5 px-2 font-mono text-neutral-900">
                          {val !== undefined ? formatNumber(val) : '\u2014'}
                        </td>
                      )
                    })}
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </div>
      )}
    </div>
  )
}

function formatNumber(n: number): string {
  if (Math.abs(n) >= 1000) {
    return n.toLocaleString('en-US', { maximumFractionDigits: 0 })
  }
  if (Math.abs(n) < 1) {
    return n.toFixed(2)
  }
  return n.toLocaleString('en-US', { maximumFractionDigits: 2 })
}

export default App
