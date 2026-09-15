// Zoom verification of specific claimed defects at 2x scale.
import ZAI from 'z-ai-web-dev-sdk';
import sharp from 'sharp';
import fs from 'fs';

const MODEL = 'glm-4.6v';

const CHECKS = [
  {
    tag: 'arb1_tradetape',
    img: '/home/z/my-project/download/arb1_eth_full.png',
    crop: { left: 860, top: 170, width: 580, height: 980 },
    prompt: `This is a 2x-zoomed crop of the TRADE TAPE panel (columns: TIME, PRICE, SIZE, VENUE) from a dark trading terminal. Answer precisely:
1) Transcribe the first 6 rows exactly (time, price, size, venue).
2) Is the PRICE column text overlapping, colliding with, or bleeding into the SIZE column on ANY row? Or is each value cleanly inside its column with clear whitespace?
3) Is any row clipped/cut off (top or bottom) in a way that looks like a rendering bug rather than normal scroll?
4) Are the decimal precisions consistent across the price values? List any outliers.
Be literal: only report what is visibly true. End with verdict: OVERLAP CONFIRMED or NO OVERLAP, and CLIPPING CONFIRMED or NO CLIPPING.`,
  },
  {
    tag: 'arb1_venuebooks',
    img: '/home/z/my-project/download/arb1_eth_full.png',
    crop: { left: 0, top: 2440, width: 1440, height: 931 },
    prompt: `This is a 2x-zoomed crop of the bottom "VENUE BOOKS" 3x3 grid (9 venue tiles, each with BID, ASK, BID SZ, LEVELS and a msg/s stat) from a dark trading terminal. Answer precisely:
1) For the Kraken tile: transcribe the exact BID SZ value. Is it clipped/cut off at the right edge of its cell, or does it fit? Does it overlap the LEVELS column?
2) For the Coinbase tile: transcribe BID SZ. Clipped or fits?
3) For the OKX tile: what exact placeholder text is shown for BID/ASK/BID SZ? Is there a "NO FEED" badge?
4) For the Gate tile: what is shown for BID/ASK/BID SZ/LEVELS? Is there a msg/s value even though price data is missing?
5) Any other tile where numbers are clipped, overlapping, or misaligned?
End with verdict lines: KRAKEN CLIPPED: yes/no; COINBASE CLIPPED: yes/no; GATE INCONSISTENCY: yes/no; OTHER DEFECTS: list or none.`,
  },
  {
    tag: 'arb3_book_sum',
    img: '/home/z/my-project/download/arb3_config.png',
    crop: { left: 0, top: 150, width: 900, height: 1050 },
    prompt: `This is a 2x-zoomed crop of the consolidated ORDER BOOK ladder (columns PRICE, SIZE, SUM, plus venue badges on the right) from a dark trading terminal. Answer precisely:
1) Transcribe 3 rows that have the largest SIZE or SUM values.
2) Do any numeric values overflow their column and overlap the next column or the venue badges? Or do all values fit cleanly?
3) Do the venue badges on the right form a clean aligned column, or do they zigzag/misalign?
4) Any text clipped at the right edge of the panel?
End with verdict: OVERFLOW CONFIRMED or NO OVERFLOW; BADGES MISALIGNED: yes/no.`,
  },
  {
    tag: 'arb4_tradetape_binance',
    img: '/home/z/my-project/download/arb4_binance_tab.png',
    crop: { left: 860, top: 170, width: 580, height: 980 },
    prompt: `This is a 2x-zoomed crop of the TRADE TAPE panel (columns TIME, PRICE, SIZE, VENUE) from a dark trading terminal (Binance venue tab selected). Answer precisely:
1) Transcribe the first 6 rows and the last 3 visible rows exactly.
2) Is any timestamp or price clipped at the left edge or cut off at the bottom in a way that looks like a rendering bug (not normal scroll fade)?
3) Do any values overlap between columns?
4) List all distinct venue tags seen.
End with verdict: CLIPPING CONFIRMED or NO CLIPPING; OVERLAP CONFIRMED or NO OVERLAP.`,
  },
  {
    tag: 'arb1_header',
    img: '/home/z/my-project/download/arb1_eth_full.png',
    crop: { left: 0, top: 0, width: 1440, height: 140 },
    prompt: `This is a 2x-zoomed crop of the top header bar of a trading terminal showing venue connection badges. Transcribe EVERY badge exactly: venue name + status word (e.g. live / connecting). How many badges total? How many say "live"? Any other status pills (e.g. WS OPEN)? Any badge text truncated or wrapped to two lines?`,
  },
];

function b64(f) { return `data:image/png;base64,${fs.readFileSync(f).toString('base64')}`; }

async function ask(zai, url, prompt) {
  const r = await zai.chat.completions.createVision({
    model: MODEL,
    messages: [{ role: 'user', content: [ { type: 'text', text: prompt }, { type: 'image_url', image_url: { url } } ] }],
    thinking: { type: 'enabled' },
  });
  return r.choices?.[0]?.message?.content ?? 'ERROR no content';
}

async function main() {
  const zai = await ZAI.create();
  const out = '/home/z/my-project/scripts/vlm_out_zoom_verify.md';
  fs.writeFileSync(out, '# Zoom verification (2x) of claimed defects\n\n');
  for (const c of CHECKS) {
    const meta = await sharp(c.img).metadata();
    const w = Math.min(c.crop.width, meta.width - c.crop.left);
    const h = Math.min(c.crop.height, meta.height - c.crop.top);
    const f = `/home/z/my-project/scripts/tmp_zoom_${c.tag}.png`;
    await sharp(c.img).extract({ left: c.crop.left, top: c.crop.top, width: w, height: h })
      .resize({ width: Math.round(w * 2) })
      .toFile(f);
    console.log(`[run] ${c.tag} (${w}x${h} @2x) ...`);
    const t0 = Date.now();
    let txt;
    try { txt = await ask(zai, b64(f), c.prompt); } catch (e) { txt = `ERROR: ${e?.message || e}`; }
    fs.appendFileSync(out, `## ${c.tag}\n\n${txt}\n\n---\n\n`);
    fs.rmSync(f, { force: true });
    console.log(`[done] ${c.tag} in ${((Date.now() - t0) / 1000).toFixed(0)}s`);
  }
  console.log('[ok] ' + out);
}
main().catch((e) => { console.error('FATAL', e); process.exit(1); });
