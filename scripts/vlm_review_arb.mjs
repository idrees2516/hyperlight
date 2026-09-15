// VLM QA review of arb trading-terminal screenshots.
// Usage: node scripts/vlm_review_arb.mjs <image.png> <outPrefix>
// Does 1 full-image pass + 3 overlapping segment passes (tall 1440px full-page shots).
import ZAI from 'z-ai-web-dev-sdk';
import sharp from 'sharp';
import fs from 'fs';
import path from 'path';

const IMG = process.argv[2];
const OUT = process.argv[3] || path.basename(IMG, '.png');
const MODEL = 'glm-4.6v';

const SEGS = [
  { name: 'segment-1 (top ~0-1250px: header, venue badges, top panels)', top: 0, height: 1250 },
  { name: 'segment-2 (middle ~1150-2400px)', top: 1150, height: 1250 },
  { name: 'segment-3 (bottom ~2300px-end)', top: 2300, height: 1400 },
];

const FULL_PROMPT = `You are a strict, critical UI QA reviewer inspecting ONE full-page screenshot of a desktop trading terminal (1440px wide, dark shadcn "zinc" theme, full-page capture so it is very tall). Produce a structured report with EXACTLY these sections:

1) HEADER VENUE BADGES
- List EVERY venue badge/pill visible in the top header, with its exact name and its status text (e.g. "live", "connecting", "down", "error"). 
- State the total count of badges and how many say "live".
- Expected venue set: Hyperliquid, Lighter, Binance, Bybit, OKX, Kraken, Coinbase, Bitstamp, Gate (9 total). Say if any are missing/extra.

2) CROSS-VENUE ARBITRAGE ENGINE CARD
- Does a card titled "Cross-Venue Arbitrage Engine" (or similar) exist? 
- For EACH of the following, say PRESENT or ABSENT, and if present transcribe the visible values/labels: stat tiles (Net P&L, Fills, Record, Best Edge, Avg Edge, Open Opps), a "Live Opportunities" table (transcribe column headers and 1-2 rows), a "Paper Equity" chart, an "Execution Log", an "Engine Configuration" form with fee input fields.

3) CORE PANELS
- Orderbook ladder, trade tape, alerts panel, depth history chart: is each rendered without visual glitches? Look specifically for: overlapping text, cut-off/clipped panels, empty panels, missing numbers (NaN, 0.00 placeholders, blank cells). Name the exact panel and location of any glitch.

4) LAYOUT PROBLEMS
- Horizontal overflow (content wider than viewport / horizontal scrollbar), misaligned table columns, unreadable/too-small text, elements colliding. Be specific with locations.

5) VERDICT
- Letter grade A-F for this screenshot, then a numbered defect list with exact locations (use panel names + approximate position like "top-right of X panel"). If genuinely defect-free, say so. Do NOT invent defects you cannot actually see, but do NOT overlook real ones.`;

const SEG_PROMPT = `You are a strict UI QA reviewer. This image is ONE VERTICAL SEGMENT (a crop) of a tall full-page screenshot of a 1440px-wide dark trading terminal (shadcn zinc theme). It is: SEGMENT POSITION_PLACEHOLDER. Inspect it at fine detail and report:
- Transcribe all venue badges/pills and their status text (e.g. "live") if visible in this segment, with a count.
- Transcribe any stat tiles with their labels and values (e.g. Net P&L, Fills, Record, Best Edge, Avg Edge, Open Opps).
- Transcribe table headers of any tables visible (e.g. Live Opportunities, Execution Log) and note if rows have real data or are empty/placeholder.
- Note any charts visible (Paper Equity, depth history) and whether they render correctly.
- Note any form fields visible (Engine Configuration, fee inputs) and their labels.
- List ALL visual defects in this segment: overlapping text, text clipped/cut off, empty panels, missing numbers/NaN, misaligned columns, elements colliding, unreadable text, cut-off panels at edges. Give exact locations.
- End with: number of venue badges seen here, how many say "live", and 2-5 most notable defects (or "none").
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
  fs.writeFileSync(outPath, `# VLM review: ${IMG} (${meta.width}x${H})\n\n`);
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
      txt = await ask(zai, b64(segFile), SEG_PROMPT.replace('POSITION_PLACEHOLDER', seg.name));
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
