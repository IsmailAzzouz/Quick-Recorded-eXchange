import init, { packets_from_file, QrSymbol, QrxReceiver } from '../../crate/pkg/qrx_web_core.js';
import jsQR from 'jsqr';
import './style.css';

const $ = (id) => document.getElementById(id);
let packets = [];
let packetIndex = 0;
let timer = null;
let receiver;
let stream = null;
let videoUrl = null;
let generation = 0;
let lastMediaTime = -1;
const video = $('incoming');
const scanCanvas = document.createElement('canvas');
const scanContext = scanCanvas.getContext('2d', { willReadFrequently: true });
const outCanvas = $('qr');
const outContext = outCanvas.getContext('2d');

function message(target, content, isError = false) {
  $(target).textContent = content;
  $(target).classList.toggle('error', isError);
}

function currentFps() {
  return Math.max(1, Math.min(30, Number($('fps').value) || 10));
}

function stopPlayback() {
  clearInterval(timer);
  timer = null;
  $('play').textContent = 'Play';
}

function renderPacket() {
  if (!packets.length) return;
  const symbol = new QrSymbol(packets[packetIndex], Number($('ecc').value));
  try {
    const size = symbol.size;
    const modules = symbol.modules();
    const scale = Math.floor(Math.min(outCanvas.width, outCanvas.height) / size);
    if (scale < 1) throw new Error('Canvas cannot fit this QR symbol');
    const marginX = Math.floor((outCanvas.width - size * scale) / 2);
    const marginY = Math.floor((outCanvas.height - size * scale) / 2);
    outContext.fillStyle = '#ffffff';
    outContext.fillRect(0, 0, outCanvas.width, outCanvas.height);
    outContext.fillStyle = '#000000';
    for (let y = 0; y < size; y++) {
      for (let x = 0; x < size; x++) {
        if (modules[y * size + x]) {
          outContext.fillRect(marginX + x * scale, marginY + y * scale, scale, scale);
        }
      }
    }
    message('tx-status', 'QR ' + (packetIndex + 1) + ' / ' + packets.length +
      ' · FQ1 · ' + currentFps() + ' FPS');
  } finally {
    symbol.free();
  }
}

function advance(amount) {
  if (!packets.length) return;
  packetIndex = (packetIndex + amount + packets.length) % packets.length;
  try { renderPacket(); } catch (err) {
    stopPlayback();
    message('tx-status', String(err), true);
  }
}

$('send-file').addEventListener('change', async (event) => {
  stopPlayback();
  packets = [];
  $('play').disabled = $('prev').disabled = $('next').disabled = true;
  const file = event.target.files?.[0];
  if (!file) return;
  if (file.size > 16 * 1024 * 1024) {
    message('tx-status', 'Browser MVP limit: 16 MiB per file.', true);
    return;
  }
  try {
    const bytes = new Uint8Array(await file.arrayBuffer());
    packets = Array.from(packets_from_file(file.name, bytes, Number($('chunk-size').value)));
    packetIndex = 0;
    renderPacket();
    $('play').disabled = $('prev').disabled = $('next').disabled = false;
  } catch (err) { message('tx-status', String(err), true); }
});
$('chunk-size').addEventListener('change', () => {
  if ($('send-file').files?.length) $('send-file').dispatchEvent(new Event('change'));
});
$('ecc').addEventListener('change', () => {
  stopPlayback();
  try { renderPacket(); } catch (err) { message('tx-status', String(err), true); }
});
$('fps').addEventListener('change', () => {
  if (timer) { stopPlayback(); $('play').click(); }
  else if (packets.length) renderPacket();
});
$('play').addEventListener('click', () => {
  if (timer) { stopPlayback(); return; }
  if (!packets.length) return;
  timer = setInterval(() => advance(1), 1000 / currentFps());
  $('play').textContent = 'Pause';
});
$('prev').addEventListener('click', () => { stopPlayback(); advance(-1); });
$('next').addEventListener('click', () => { stopPlayback(); advance(1); });

function safeFilename(name) {
  const clean = name.replace(/[\\/\x00-\x1f<>:"|?*]+/g, '_').replace(/^\.+/, '').trim();
  return clean.slice(0, 180) || 'recovered.bin';
}
function refreshReceived() {
  const list = $('received');
  list.replaceChildren();
  for (let i = 0; i < receiver.file_count(); i++) {
    const card = document.createElement('article');
    card.className = 'received-card';
    const title = document.createElement('strong');
    title.textContent = receiver.filename(i);
    const stats = document.createElement('p');
    const current = receiver.received_parts(i);
    const total = receiver.total_parts(i);
    stats.textContent = current + ' / ' + total + ' packets · ' +
      (receiver.is_complete(i) ? 'SHA-256 verified' : 'Waiting for frames');
    const progress = document.createElement('progress');
    progress.max = total;
    progress.value = current;
    card.append(title, stats, progress);
    const verificationError = receiver.verification_error(i);
    if (verificationError) {
      const detail = document.createElement('p');
      detail.className = 'error';
      detail.textContent = verificationError;
      card.append(detail);
    }
    if (receiver.is_complete(i)) {
      const button = document.createElement('button');
      button.textContent = 'Download verified file';
      button.addEventListener('click', () => {
        const bytes = receiver.verified_bytes(i);
        const blob = new Blob([bytes], { type: 'application/octet-stream' });
        const url = URL.createObjectURL(blob);
        const link = document.createElement('a');
        link.href = url;
        link.download = safeFilename(receiver.filename(i));
        link.click();
        setTimeout(() => URL.revokeObjectURL(url), 30000);
      });
      card.append(button);
    }
    list.append(card);
  }
}
function scanFrame() {
  if (video.readyState < HTMLMediaElement.HAVE_CURRENT_DATA || !video.videoWidth) return;
  const width = video.videoWidth;
  const height = video.videoHeight;
  scanCanvas.width = width;
  scanCanvas.height = height;
  scanContext.drawImage(video, 0, 0, width, height);
  const frame = scanContext.getImageData(0, 0, width, height);
  const result = jsQR(frame.data, width, height, { inversionAttempts: 'dontInvert' });
  if (!result?.data?.startsWith('FQ1|')) return;
  try {
    if (receiver.ingest(result.data)) {
      refreshReceived();
      message('rx-status', 'Received ' + receiver.file_count() + ' transfer(s).');
    }
  } catch (err) { message('rx-status', String(err), true); }
}
function scheduleScan(token) {
  if (generation !== token) return;
  if (video.requestVideoFrameCallback) {
    video.requestVideoFrameCallback((_now, metadata) => {
      if (generation !== token) return;
      if (metadata.mediaTime !== lastMediaTime) {
        lastMediaTime = metadata.mediaTime;
        scanFrame();
      }
      scheduleScan(token);
    });
  } else {
    // Compatibility fallback for browsers without requestVideoFrameCallback.
    setTimeout(() => {
      if (generation !== token) return;
      if (!video.paused) scanFrame();
      scheduleScan(token);
    }, 100);
  }
}
function stopScan() {
  generation++;
  lastMediaTime = -1;
  video.pause();
  video.srcObject = null;
  video.removeAttribute('src');
  video.load();
  if (stream) {
    stream.getTracks().forEach((track) => track.stop());
    stream = null;
  }
  if (videoUrl) { URL.revokeObjectURL(videoUrl); videoUrl = null; }
}
$('stop').addEventListener('click', () => {
  stopScan();
  message('rx-status', 'Capture stopped.');
});
$('clear').addEventListener('click', () => {
  receiver = new QrxReceiver();
  refreshReceived();
  message('rx-status', 'Received transfers cleared.');
});
$('camera').addEventListener('click', async () => {
  stopScan();
  if (!navigator.mediaDevices?.getUserMedia) {
    message('rx-status', 'Camera requires HTTPS or localhost and a compatible browser.', true);
    return;
  }
  const token = generation;
  try {
    const acquired = await navigator.mediaDevices.getUserMedia({
      video: { facingMode: { ideal: 'environment' } }, audio: false
    });
    if (token !== generation) {
      acquired.getTracks().forEach((track) => track.stop());
      return;
    }
    stream = acquired;
    video.srcObject = stream;
    video.removeAttribute('controls');
    await video.play();
    message('rx-status', 'Scanning camera locally…');
    scheduleScan(generation);
  } catch (err) { message('rx-status', 'Cannot start camera: ' + err, true); }
});
$('receive-file').addEventListener('change', async (event) => {
  stopScan();
  const file = event.target.files?.[0];
  if (!file) return;
  videoUrl = URL.createObjectURL(file);
  video.src = videoUrl;
  video.setAttribute('controls', '');
  try {
    await video.play();
    message('rx-status', 'Scanning local video; browser codec support varies.');
    scheduleScan(generation);
  } catch (err) { message('rx-status', 'Cannot play video: ' + err, true); }
});
video.addEventListener('ended', () => message('rx-status', 'Video ended. Replay if packets are missing.'));

try {
  await init();
  receiver = new QrxReceiver();
  outContext.fillStyle = '#fff';
  outContext.fillRect(0, 0, outCanvas.width, outCanvas.height);
  message('tx-status', 'Ready. Select a file to generate QR frames.');
  if ('serviceWorker' in navigator && location.protocol !== 'file:') {
    navigator.serviceWorker.register('/sw.js').catch(() => {});
  }
} catch (err) {
  message('tx-status', 'Cannot load WebAssembly: ' + err, true);
  message('rx-status', 'Cannot load WebAssembly: ' + err, true);
  $('send-file').disabled = $('receive-file').disabled = $('camera').disabled = true;
}
