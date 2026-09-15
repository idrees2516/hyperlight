// Objective pixel analysis of venue-book tiles across all 4 screenshots:
//  - detect tile grid bands (card-bg rows)
//  - per tile: transcribe text-cluster layout of labels row + values row
//  - measure gaps (BID SZ end -> LEVELS start, LEVELS end -> tile right edge)
//  - detect 1px white vertical lines (possible border artifact)
import sharp from 'sharp';

const lum = (p) => 0.299*p[0] + 0.587*p[1] + 0.114*p[2];
const CARD = [13,13,16];
const close = (p,q,tol) => Math.abs(p[0]-q[0])<=tol && Math.abs(p[1]-q[1])<=tol && Math.abs(p[2]-q[2])<=tol;

const NAMES = { 1:'Hyperliquid', 2:'Lighter', 3:'Binance', 4:'Bybit', 5:'OKX', 6:'Kraken', 7:'Coinbase', 8:'Bitstamp', 9:'Gate' };
const TILE_COLS = [[105,505],[520,920],[935,1335]];

async function getRaw(img, x0, y0, w, h) {
  const { data, info } = await sharp(img).extract({ left: x0, top: y0, width: w, height: h }).raw().toBuffer({ resolveWithObject: true });
  return { data, info };
}

async function analyzeImage(img) {
  const meta = await sharp(img).metadata();
  const H = meta.height;
  console.log(`\n########## ${img} (${meta.width}x${H})`);

  // 1) find tile row bands near the bottom
  const rTop = H - 660;
  const { data: dR, info: iR } = await getRaw(img, 0, rTop, 1440, H - rTop);
  const Wr = iR.width, Hr = iR.height, chr = iR.channels;
  const isCard = (x, y) => { const o = (y*Wr+x)*chr; return close([dR[o],dR[o+1],dR[o+2]], CARD, 3); };
  let bands = [], ys = null;
  for (let y = 0; y < Hr; y++) {
    let c = 0; for (let x = 115; x < 1335; x += 3) if (isCard(x, y)) c++;
    const f = c / ((1335-115)/3);
    if (f > 0.7) { if (ys === null) ys = y; } else { if (ys !== null) { bands.push([ys+rTop, y+rTop]); ys = null; } }
  }
  if (ys !== null) bands.push([ys+rTop, Hr+rTop]);
  // merge bands separated by small dark divider lines (<14px)
  let mbands = [];
  for (const b of bands) {
    if (mbands.length && b[0] - mbands[mbands.length-1][1] <= 14) mbands[mbands.length-1][1] = b[1];
    else mbands.push([...b]);
  }
  console.log('tile row bands (abs y):', JSON.stringify(mbands));

  // 2) per-tile cluster analysis
  for (let bi = 0; bi < mbands.length && bi < 3; bi++) {
    const [by0, by1] = mbands[bi];
    // find text lines within tile band
    for (let ci = 0; ci < 3; ci++) {
      const [x0, x1] = TILE_COLS[ci];
      const idx = bi*3 + ci + 1;
      const { data, info } = await getRaw(img, x0, by0, x1 - x0, by1 - by0);
      const W = info.width, Hh = info.height, ch = info.channels;
      const px = (x,y) => { const o=(y*W+x)*ch; return [data[o],data[o+1],data[o+2]]; };
      // text lines: rows with >4 text pixels, ignoring rightmost 3 px (white border line)
      let lines = [], ls = null;
      for (let y = 0; y < Hh; y++) {
        let c = 0; for (let x = 0; x < W - 4; x++) if (lum(px(x,y)) > 75) c++;
        if (c > 4) { if (ls === null) ls = y; } else { if (ls !== null) { if (y - 1 - ls >= 5) lines.push([ls, y-1]); ls = null; } }
      }
      if (ls !== null && Hh - 1 - ls >= 5) lines.push([ls, Hh-1]);
      const desc = [];
      for (const [ly0, ly1] of lines) {
        let clusters = [], s = null;
        for (let x = 0; x < W - 4; x++) {
          let c = 0; for (let y = ly0; y <= ly1; y++) if (lum(px(x,y)) > 75) c++;
          if (c >= 1) { if (s === null) s = x; } else { if (s !== null) { clusters.push([s, x-1]); s = null; } }
        }
        if (s !== null) clusters.push([s, W-5]);
        let merged = [];
        for (const c of clusters) { if (merged.length && c[0]-merged[merged.length-1][1] <= 2) merged[merged.length-1][1] = c[1]; else merged.push([...c]); }
        const gaps = [];
        for (let i = 1; i < merged.length; i++) gaps.push(merged[i][0]-merged[i-1][1]-1);
        desc.push(`line y${ly0+by0}-${ly1+by0}: ${merged.map(m=>m[0]+'-'+m[1]).join(' ')} gaps:[${gaps.join(',')}]`);
      }
      console.log(`-- tile r${bi+1}c${ci+1} (${NAMES[idx]}) x[${x0},${x1}] y[${by0},${by1}]`);
      for (const d of desc) console.log('   ' + d);
    }
  }

  // 3) white vertical line detection across whole page (sample rows)
  const found = {};
  for (const yProbe of [200, 600, 1000, 1500, 2000, 2500, H-300]) {
    const { data, info } = await getRaw(img, 0, yProbe, 1440, 1);
    const ch = info.channels;
    for (let x = 0; x < 1440; x++) {
      const o = x*ch;
      if (data[o] > 230 && data[o+1] > 230 && data[o+2] > 230) {
        found[yProbe] = found[yProbe] || [];
        if (found[yProbe].length && x - found[yProbe][found[yProbe].length-1][1] <= 2) found[yProbe][found[yProbe].length-1][1] = x;
        else found[yProbe].push([x, x]);
      }
    }
  }
  console.log('white vertical lines by probe row:', JSON.stringify(found));
}

const files = process.argv.slice(2);
for (const f of files) await analyzeImage(f);
