import sharp from 'sharp';
const lum = (p) => 0.299*p[0]+0.587*p[1]+0.114*p[2];
const img = 'download/arb1_eth_full.png';
const { data, info } = await sharp(img).extract({ left: 105, top: 255, width: 814, height: 130 }).raw().toBuffer({ resolveWithObject: true });
const W = info.width, H = info.height, ch = info.channels;
const px = (x, y) => { const o = (y*W+x)*ch; return [data[o], data[o+1], data[o+2]]; };
let lines = [], ls = null;
for (let y = 0; y < H; y++) {
  let c = 0; for (let x = 0; x < W; x++) if (lum(px(x,y)) > 75) c++;
  if (c > 3) { if (ls === null) ls = y; } else { if (ls !== null) { lines.push([ls, y-1]); ls = null; } }
}
if (ls !== null) lines.push([ls, H-1]);
console.log('lines (rel y in 255-385): ' + lines.map(l => (l[0]+255)+'-'+(l[1]+255)).join(' | '));
for (const [ly0, ly1] of lines) {
  let clusters = [], s = null;
  for (let x = 0; x < W; x++) {
    let c = 0; for (let y = ly0; y <= ly1; y++) if (lum(px(x,y)) > 75) c++;
    if (c >= 1) { if (s === null) s = x; } else { if (s !== null) { clusters.push([s, x-1]); s = null; } }
  }
  if (s !== null) clusters.push([s, W-1]);
  let merged = [];
  for (const c of clusters) { if (merged.length && c[0]-merged[merged.length-1][1] <= 3) merged[merged.length-1][1] = c[1]; else merged.push([...c]); }
  const last = merged[merged.length-1];
  console.log('  line y'+(ly0+255)+'-'+(ly1+255)+': rightmost text ends at x='+(last[1]+105)+' gap to panel right edge 919 = '+(919-last[1]-105)+' | clusters: '+merged.map(m => (m[0]+105)+'-'+(m[1]+105)).join(' '));
}
