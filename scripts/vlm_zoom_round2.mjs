// Round-2 zoom verification: tight crops at high zoom, neutral evidence-first prompts.
import ZAI from 'z-ai-web-dev-sdk';
import sharp from 'sharp';
import fs from 'fs';

const MODEL = 'glm-4.6v';

const CHECKS = [
  {
    tag: 'arb2_kraken_tile_3x',
    img: '/home/z/my-project/download/arb2_sol_full.png',
    crop: { left: 935, top: 3043, width: 400, height: 122 },
    scale: 3,
    prompt: `High-zoom crop of ONE venue order-book tile ("Kraken") from a dark trading terminal. It has a header line (venue name, a KR badge, a right-side stat), then a small table with columns BID, ASK, BID SZ, LEVELS, then one more small line at the bottom.
Answer precisely, transcribing character-by-character:
1) Transcribe the FULL values row exactly: BID=?, ASK=?, BID SZ=?, LEVELS=?.
2) Count the digits of the BID SZ value. Is its LAST digit fully drawn (complete glyph with normal right-side bearing), or is it partially cut/sliced at its right edge?
3) Describe the whitespace gap between the end of the BID SZ value and the start of the LEVELS value: roughly how many characters wide is it?
4) What does the small bottom line of the tile say, exactly?
5) Any text in this tile cut off, overlapping, or touching a border? Be literal.
End with verdict lines: BID SZ LAST DIGIT: complete|partially-cut; GAP BID SZ->LEVELS: wide|narrow|none; OTHER CLIPPING: none|<describe>.`,
  },
  {
    tag: 'arb2_coinbase_tile_3x',
    img: '/home/z/my-project/download/arb2_sol_full.png',
    crop: { left: 105, top: 3179, width: 400, height: 122 },
    scale: 3,
    prompt: `High-zoom crop of ONE venue order-book tile ("Coinbase") from a dark trading terminal. It has a header line (venue name, a CB badge, a right-side stat), then a small table with columns BID, ASK, BID SZ, LEVELS, then one more small line at the bottom.
Answer precisely, transcribing character-by-character:
1) Transcribe the FULL values row exactly: BID=?, ASK=?, BID SZ=?, LEVELS=?.
2) Count the digits of the BID SZ value. Is its LAST digit fully drawn (complete glyph), or partially cut/sliced at its right edge?
3) Describe the whitespace gap between the end of BID SZ and the start of LEVELS.
4) What does the small bottom line say, exactly?
5) Any text cut off, overlapping, or touching a border in this tile? Be literal.
End with verdict lines: BID SZ LAST DIGIT: complete|partially-cut; GAP BID SZ->LEVELS: wide|narrow|none; OTHER CLIPPING: none|<describe>.`,
  },
  {
    tag: 'arb1_gate_tile_3x',
    img: '/home/z/my-project/download/arb1_eth_full.png',
    crop: { left: 935, top: 3179, width: 400, height: 122 },
    scale: 3,
    prompt: `High-zoom crop of ONE venue order-book tile ("Gate") from a dark trading terminal. Columns: BID, ASK, BID SZ, LEVELS.
1) Transcribe the values row exactly: BID=?, ASK=?, BID SZ=?, LEVELS=?.
2) Are the BID/ASK values real numbers or placeholder dashes?
3) Is LEVELS a number greater than zero?
4) Does the literal word "waiting" appear anywhere in this tile?
5) Any text clipped or overlapping?
End with verdict lines: GATE DATA: real|dashes; LEVELS>0: yes|no; WAITING TEXT: found|not-found; CLIPPING: none|<describe>.`,
  },
  {
    tag: 'arb1_tape_bottom_3x',
    img: '/home/z/my-project/download/arb1_eth_full.png',
    crop: { left: 800, top: 830, width: 540, height: 120 },
    scale: 3,
    prompt: `High-zoom crop of the BOTTOM EDGE of a "Trade Tape" panel (columns TIME, PRICE, SIZE, VENUE) from a dark trading terminal, plus the beginning of whatever panel/content follows below it.
1) Transcribe the last 3 rows of the tape exactly (time, price, size, notional, venue).
2) Is the BOTTOM-MOST tape row fully drawn (all characters complete with full letter height and descenders), or is it sliced mid-character by the panel's bottom edge?
3) Is there visible empty space / padding between the last row and the panel border, or does the text touch the border?
4) Describe what content appears below the tape panel in this crop.
End with verdict lines: BOTTOM ROW: fully-visible|cut-mid-character; PADDING BELOW LAST ROW: present|absent.`,
  },
  {
    tag: 'arb4_binance_tape_2x',
    img: '/home/z/my-project/download/arb4_binance_tab.png',
    crop: { left: 800, top: 380, width: 540, height: 460 },
    scale: 2,
    prompt: `High-zoom crop of a "Trade Tape" panel (columns TIME, PRICE, SIZE, VENUE) from a dark trading terminal, Binance venue tab selected.
1) Transcribe the PRICE column values of the first 10 rows EXACTLY, character by character, including every decimal digit. Do not round or normalize.
2) Do ANY of the price values contain more than 4 decimal places or long trailing zeros like 100.95000000 or 2475.26000000? List any that do.
3) Do any values overlap between columns?
End with verdict lines: TRAILING-ZERO PRICES: found|not-found; LONGEST PRICE STRING: <write it>.`,
  },
  {
    tag: 'arb1_header_badges_2x',
    img: '/home/z/my-project/download/arb1_eth_full.png',
    crop: { left: 0, top: 0, width: 1440, height: 130 },
    scale: 2,
    prompt: `High-zoom crop of the top header bar of a trading terminal showing venue connection badges.
1) Transcribe EVERY badge exactly: venue name + status word.
2) How many badges total, and how many say "live"?
3) What does the OKX badge say exactly?
4) Any badge text truncated, wrapped, or clipped?
End with verdict lines: BADGE COUNT: N; LIVE COUNT: N; OKX STATUS: <text>.`,
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
  const out = '/home/z/my-project/scripts/vlm_out_zoom_round2.md';
  fs.writeFileSync(out, '# Round-2 zoom verification (tight crops, neutral prompts)\n\n');
  for (const c of CHECKS) {
    const meta = await sharp(c.img).metadata();
    const w = Math.min(c.crop.width, meta.width - c.crop.left);
    const h = Math.min(c.crop.height, meta.height - c.crop.top);
    const f = `/home/z/my-project/scripts/tmp_z2_${c.tag}.png`;
    await sharp(c.img).extract({ left: c.crop.left, top: c.crop.top, width: w, height: h })
      .resize({ width: Math.round(w * c.scale) })
      .toFile(f);
    console.log(`[run] ${c.tag} (${w}x${h} @${c.scale}x) ...`);
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
