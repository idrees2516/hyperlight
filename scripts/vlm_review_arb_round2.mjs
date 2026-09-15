// VLM QA RE-REVIEW (round 2, after bug fixes) of arb trading-terminal screenshots.
// Usage: node scripts/vlm_review_arb_round2.mjs <image.png> <outPrefix>
// Does 1 full-image pass + 3 overlapping vertical segment passes (tall 1440px full-page shots).
// Round-2 focus: verify 6 previously-reported defects are FIXED, plus overall completeness.
import ZAI from 'z-ai-web-dev-sdk';
import sharp from 'sharp';
import fs from 'fs';
import path from 'path';

const IMG = process.argv[2];
const OUT = process.argv[3] || path.basename(IMG, '.png');
const MODEL = 'glm-4.6v';

const SEGS = [
  { name: 'segment-1 (top ~0-1250px: header, venue badges, venue book tiles, top panels)', top: 0, height: 1250 },
  { name: 'segment-2 (middle ~1150-2400px: ladder/tape/engine panels)', top: 1150, height: 1250 },
  { name: 'segment-3 (bottom ~2300px-end: config form, execution log, charts)', top: 2300, height: 1400 },
];

const DEFECT_CHECKLIST = `FOCUS — SIX PREVIOUSLY-REPORTED DEFECTS. For each, look closely and give a verdict FIXED or STILL BROKEN, quoting what you actually see:
[a] Venue Books tiles (small order-book tiles per venue, e.g. Hyperliquid/Lighter/Binance/Bybit/OKX/Kraken/Coinbase/Bitstamp/Gate): the "BID SZ" column text used to be CLIPPED at the right edge of the cell (especially Kraken and Coinbase tiles). Is every size value now fully visible with no cut-off digits?
[b] The OKX badge in the header used to be stuck on "connecting". What does the OKX badge say now — "live", "connecting", or something else?
[c] The Gate venue tile used to show all dashes and "LEVELS 0". Does the Gate tile now show real prices and LEVELS > 0? Transcribe its top-of-book prices and LEVELS value.
[d] Binance trade tape prices used to render with raw 8-decimal trailing zeros like "2475.26000000". Transcribe 3-5 actual price values from the trade tape(s) exactly as shown. Are they trimmed (e.g. "2475.26") or still showing long trailing zeros?
[e] The trade tape bottom row used to be clipped mid-character (cut off at the bottom edge of the panel). Is the last row of the tape now fully visible?
[f] Venue tiles used to show the literal text "waiting..." while loading. Search every venue tile for the literal string "waiting" — is it gone (replaced by an em-dash "—" or real data)? Quote any occurrence you find.`;

const FULL_PROMPT = `You are a strict, critical UI QA reviewer doing a SECOND-ROUND re-review (after bug fixes) of ONE full-page screenshot of a desktop trading terminal (1440px wide, dark shadcn "zinc" theme, full-page capture so it is very tall). Produce a structured report with EXACTLY these sections:

1) HEADER VENUE BADGES
- List EVERY venue badge/pill in the top header with exact name and status text (e.g. "live", "connecting", "down", "error"). Give total count and how many say "live".
- Expected venue set: Hyperliquid, Lighter, Binance, Bybit, OKX, Kraken, Coinbase, Bitstamp, Gate (9 total). Note any missing/extra.

2) ${DEFECT_CHECKLIST}

3) CROSS-VENUE ARBITRAGE ENGINE CARD
- For EACH item say PRESENT or ABSENT and transcribe visible values/labels: stat tiles (Net P&L, Fills, Record, Best Edge, Avg Edge, Open Opps), "Live Opportunities" table (column headers + 1-2 rows), "Paper Equity" chart, "Execution Log", "Engine Configuration" form. Count the fee/taker-fee input fields in the configuration form (should be 9, one per venue) and list their labels.

4) CORE PANELS
- Orderbook ladder, trade tape(s), alerts panel, depth history chart: rendered cleanly? Look for overlapping text, clipped panels, empty panels, NaN/blank cells. Name exact panel + location of any glitch.

5) LAYOUT PROBLEMS
- Horizontal overflow, misaligned columns, colliding elements, unreadable text. Be specific.

6) VERDICT
- Letter grade A-F for this screenshot, then a numbered list of remaining defects with exact locations, then one line per defect a-f above: "a) FIXED" or "a) STILL BROKEN: <what you see>". Do NOT invent defects you cannot see; do NOT overlook real ones.`;

const SEG_PROMPT = `You are a strict UI QA reviewer doing a SECOND-ROUND re-review (after bug fixes). This image is ONE VERTICAL SEGMENT (a crop) of a tall full-page screenshot of a 1440px-wide dark trading terminal (shadcn zinc theme). It is: SEGMENT_POSITION_PLACEHOLDER. Inspect at fine detail and report:

A) VENUE BOOK TILES (if visible): for each venue tile transcribe the venue name, LEVELS count, top bid/ask prices, and the BID SZ / ASK SZ (size) column values. Explicitly state whether any size text is clipped/cut off at the cell edge (this was defect [a], worst on Kraken and Coinbase tiles). Explicitly state whether the Gate tile shows real prices and LEVELS > 0 (defect [c]) or dashes. Explicitly search for the literal text "waiting" in any tile (defect [f]) — quote it if found.

B) HEADER BADGES (if visible): transcribe every venue badge and its status text; count them; what does the OKX badge say (defect [b])?

C) TRADE TAPE(S) (if visible): transcribe 3-5 price values from the tape EXACTLY as printed — do they have long trailing zeros like 2475.26000000 (defect [d]) or are they trimmed like 2475.26? Is the BOTTOM-most row of the tape fully visible, or is it cut off mid-character at the panel edge (defect [e])?

D) OTHER CONTENT: stat tiles with labels+values, table headers (Live Opportunities, Execution Log) and whether rows have real data, charts (Paper Equity, depth history) rendering, form fields (Engine Configuration, fee inputs — count them), orderbook ladder / alerts panel state.

E) DEFECTS: list ALL visual defects in this segment (overlapping text, clipped text, empty panels, NaN, misaligned columns, collisions, unreadable text, panels cut at edges) with exact locations. End with: badge count + "live" count, and a one-line verdict for each of defects [a]-[f] that is checkable in this segment (FIXED / STILL BROKEN / NOT VISIBLE HERE).

Be critical, specific, and literal. Do not invent details.`;

function b64(file) {
  return `data:image/png;base64,${fs.readFileSync(file).toString('base64')}`;
}

async function ask(zai, dataUrl, prompt) {
  const resp = await zai.chat.completions.createVision({
    model: MODEL,
    messages: [
      {
        role: 'user',
        content: [
          { type: 'text', text: prompt },
          { type: 'image_url', image_url: { url: dataUrl } },
        ],
      },
    ],
    thinking: { type: 'enabled' },
  });
  return resp.choices?.[0]?.message?.content ?? JSON.stringify(resp).slice(0, 2000);
}

async function main() {
  const meta = await sharp(IMG).metadata();
  const H = meta.height;
  const outPath = `/home/z/my-project/scripts/vlm_out_${OUT}.md`;
  fs.writeFileSync(outPath, `# VLM round-2 re-review: ${IMG} (${meta.width}x${H})\n\n`);
  console.log(`[info] ${IMG} ${meta.width}x${H} -> ${outPath}`);

  const zai = await ZAI.create();

  // Full-image pass
  console.log('[run] full-image pass ...');
  let t0 = Date.now();
  let full;
  try {
    full = await ask(zai, b64(IMG), FULL_PROMPT);
  } catch (e) {
    full = `ERROR: ${e?.message || e}`;
  }
  fs.appendFileSync(outPath, `## FULL-PAGE PASS\n\n${full}\n\n`);
  console.log(`[done] full pass in ${((Date.now() - t0) / 1000).toFixed(0)}s`);

  // Segment passes
  for (const seg of SEGS) {
    const h = Math.min(seg.height, H - seg.top);
    if (h <= 50) continue;
    const segFile = `/home/z/my-project/scripts/tmp_${OUT}_seg${seg.top}.png`;
    await sharp(IMG).extract({ left: 0, top: seg.top, width: meta.width, height: h }).toFile(segFile);
    console.log(`[run] ${seg.name} (${meta.width}x${h}) ...`);
    t0 = Date.now();
    let txt;
    try {
      txt = await ask(zai, b64(segFile), SEG_PROMPT.replace('SEGMENT_POSITION_PLACEHOLDER', seg.name));
    } catch (e) {
      txt = `ERROR: ${e?.message || e}`;
    }
    fs.appendFileSync(outPath, `## ${seg.name}\n\n${txt}\n\n`);
    fs.rmSync(segFile, { force: true });
    console.log(`[done] segment in ${((Date.now() - t0) / 1000).toFixed(0)}s`);
  }
  console.log(`[ok] wrote ${outPath}`);
}

main().catch((e) => {
  console.error('FATAL', e);
  process.exit(1);
});
