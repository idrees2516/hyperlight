import ZAI from 'z-ai-web-dev-sdk';
import fs from 'fs';
const MODEL = 'glm-4.6v';
const b64 = (f) => `data:image/png;base64,${fs.readFileSync(f).toString('base64')}`;
const zai = await ZAI.create();
const out = '/home/z/my-project/scripts/vlm_out_sweep.md';
fs.writeFileSync(out, '# Sweep panel VLM verification\n\n');

const CHECKS = [
  { tag: 'sweep_panel', img: 'download/sweep_panel.png' },
  { tag: 'sweep_full', img: 'download/sweep_full_page.png' },
];
const prompt = `Screenshot of a dark-themed (zinc) professional trading terminal showing the "Global Sweep Optimizer" panel. Verify carefully:
1) Does the panel header read "Global Sweep Optimizer — provably optimal multi-venue execution"?
2) Stat tiles: which ones are visible (Open Plans, Sweep P&L (paper), Best Net Edge, Avg Net Edge, Avg Venues)? Any values shown?
3) The plans table: column headers should be Mkt / Optimal venue split (buy -> sell) / Net bps / EV / Profit / Notional. Are venue chips with buy/sell labels visible (e.g. "buy HL", "sell BY")?
4) The execution log at the bottom: does it have Status / Executed split / Detected / P&L / Legs / Time columns? Any fill rows?
5) Any rendering defects: clipped text, overlapping elements, empty garbled areas, missing data (— dashes ok)?
End with verdict lines: PANEL OK: yes|no; DEFECTS: none|<list>.`;

for (const c of CHECKS) {
  console.log(`[run] ${c.tag} ...`);
  const r = await zai.chat.completions.createVision({
    model: MODEL,
    messages: [{ role: 'user', content: [ { type: 'text', text: prompt }, { type: 'image_url', image_url: { url: b64(c.img) } } ] }],
    thinking: { type: 'enabled' },
  });
  const txt = r.choices[0].message.content;
  fs.appendFileSync(out, `\n## ${c.tag}\n\n${txt}\n`);
  console.log(txt.slice(-400));
}
