// Detect venue-books tile borders + tape panel borders via pixel analysis.
import sharp from 'sharp';

const BORD = [39, 39, 42]; // zinc-800 #27272a
const close = (p, q, tol) => Math.abs(p[0]-q[0])<=tol && Math.abs(p[1]-q[1])<=tol && Math.abs(p[2]-q[2])<=tol;
const lum = (p) => 0.299*p[0] + 0.587*p[1] + 0.114*p[2];

async function analyze(img) {
  const meta = await sharp(img).metadata();
  const H = meta.height;
  // --- bottom region: venue books grid ---
  const rTop = Math.max(0, H - 950);
  const { data, info } = await sharp(img).extract({ left: 0, top: rTop, width: 1440, height: H - rTop }).raw().toBuffer({ resolveWithObject: true });
  const W = info.width, Rh = info.height, ch = info.channels;
  const px = (x, y) => [data[(y*W+x)*ch], data[(y*W+x)*ch+1], data[(y*W+x)*ch+2]];

  // vertical border lines: columns where many pixels match border color
  const vLines = [];
  for (let x = 0; x < W; x++) {
    let c = 0;
    for (let y = 0; y < Rh; y += 2) if (close(px(x,y), BORD, 12)) c += 2;
    if (c > Rh * 0.45) vLines.push(x);
  }
  // horizontal border lines
  const hLines = [];
  for (let y = 0; y < Rh; y++) {
    let c = 0;
    for (let x = 0; x < W; x += 2) if (close(px(x,y), BORD, 12)) c += 2;
    if (c > W * 0.45) hLines.push(y + rTop);
  }
  console.log(`\n=== ${img} (H=${H}) region y=${rTop}-${H}`);
  console.log('vertical border x:', JSON.stringify(vLines));
  console.log('horizontal border y (abs):', JSON.stringify(hLines));

  // text pixel stats per tile column band (between vertical lines)
  const bands = [];
  for (let i = 0; i < vLines.length - 1; i++) {
    const x0 = vLines[i], x1 = vLines[i+1];
    if (x1 - x0 < 200) continue;
    bands.push([x0, x1]);
  }
  if (vLines.length && vLines[0] > 200) bands.unshift([0, vLines[0]]);
  if (vLines.length && W - vLines[vLines.length-1] > 200) bands.push([vLines[vLines.length-1], W]);
  for (const [x0, x1] of bands) {
    // rightmost text pixel in band
    let maxX = -1, maxY = -1;
    for (let x = x1 - 1; x >= x0; x--) {
      for (let y = 0; y < Rh; y++) {
        if (lum(px(x,y)) > 75) { if (x > maxX) { maxX = x; maxY = y; } break; }
      }
      if (maxX === x) break;
    }
    console.log(`band x=[${x0},${x1}] rightmost text x=${maxX} (y=${maxY === -1 ? '-' : maxY + rTop}) gapToRightBorder=${maxX === -1 ? '-' : x1 - maxX}`);
  }

  // --- top-right region: trade tape panel ---
  const { data: d2, info: i2 } = await sharp(img).extract({ left: 780, top: 120, width: 660, height: 1200 }).raw().toBuffer({ resolveWithObject: true });
  const W2 = i2.width, H2 = i2.height;
  const px2 = (x, y) => [d2[(y*W2+x)*ch2], d2[(y*W2+x)*ch2+1], d2[(y*W2+x)*ch2+2]];
  const ch2 = i2.channels;
  const hTape = [];
  for (let y = 0; y < H2; y++) {
    let c = 0;
    for (let x = 0; x < W2; x += 2) if (close(px2(x,y), BORD, 12)) c += 2;
    if (c > W2 * 0.5) hTape.push(y + 120);
  }
  const vTape = [];
  for (let x = 0; x < W2; x++) {
    let c = 0;
    for (let y = 0; y < H2; y += 2) if (close(px2(x,y), BORD, 12)) c += 2;
    if (c > H2 * 0.4) vTape.push(x + 780);
  }
  console.log('tape region (x780-1440, y120-1320) horizontal borders y:', JSON.stringify(hTape));
  console.log('tape region vertical borders x:', JSON.stringify(vTape));
}

const files = process.argv.slice(2);
for (const f of files) await analyze(f);
