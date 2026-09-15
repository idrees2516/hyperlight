import ZAI from 'z-ai-web-dev-sdk';
import sharp from 'sharp';
import fs from 'fs';
const MODEL = 'glm-4.6v';
const b64 = (f) => `data:image/png;base64,${fs.readFileSync(f).toString('base64')}`;
const zai = await ZAI.create();
const out = '/home/z/my-project/scripts/vlm_out_config_form.md';
fs.writeFileSync(out, '# Config form zoom check\n\n');

const CHECKS = [
  { tag: 'arb3_config_form', img: 'download/arb3_config.png', crop: { left: 105, top: 2000, width: 1230, height: 420 }, scale: 2 },
  { tag: 'arb1_config_form', img: 'download/arb1_eth_full.png', crop: { left: 105, top: 2000, width: 1230, height: 420 }, scale: 2 },
];
const prompt = `Zoomed crop of the "Engine Configuration" form area of a trading terminal (dark zinc theme). It should contain: engine parameter inputs (MIN EDGE (BPS), FIRE EDGE (BPS), MAX NOTIONAL ($), LATENCY (MS), COOLDOWN (MS)), a taker-fees input grid with one small labeled number input per venue, and an "Apply configuration" button.
1) List EVERY input field you can see with its exact label and current value.
2) How many taker-fee inputs are there in the fees grid, and what are their labels?
3) Is any input clipped, overlapping, or misaligned? Is the Apply button fully visible?
End with verdict lines: FEE INPUT COUNT: N; FEE LABELS: <comma list>; DEFECTS: none|<list>.`;

for (const c of CHECKS) {
  const meta = await sharp(c.img).metadata();
  const w = Math.min(c.crop.width, meta.width - c.crop.left);
  const h = Math.min(c.crop.height, meta.height - c.crop.top);
  const f = `scripts/tmp_cf_${c.tag}.png`;
  await sharp(c.img).extract({ left: c.crop.left, top: c.crop.top, width: w, height: h }).resize({ width: w * c.scale }).toFile(f);
  console.log(`[run] ${c.tag} ...`);
  const r = await zai.chat.completions.createVision({
    model: MODEL,
    messages: [{ role: 'user', content: [ { type: 'text', text: prompt }, { type: 'image_url', image_url: { url: b64(f) } } ] }],
    thinking: { type: 'enabled' },
  });
  const txt = r.choices?.[0]?.message?.content ?? 'ERROR';
  fs.appendFileSync(out, `## ${c.tag}\n\n${txt}\n\n---\n\n`);
  fs.rmSync(f, { force: true });
  console.log(`[done] ${c.tag}`);
}
console.log('[ok] ' + out);
