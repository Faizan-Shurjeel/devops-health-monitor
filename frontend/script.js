// Basic SPA using vanilla JS + Chart.js

function resolveApiBase() {
  const url = new URL(window.location.href);
  const fromQuery = url.searchParams.get('api');
  if (fromQuery) {
    localStorage.setItem('apiBase', fromQuery);
    return fromQuery.replace(/\/$/, '');
  }
  const fromStorage = localStorage.getItem('apiBase');
  if (fromStorage) return fromStorage.replace(/\/$/, '');
  return (window.location && window.location.origin) || '';
}

const API_BASE = resolveApiBase();

document.getElementById('api-base').textContent = API_BASE;

const targetsListEl = document.getElementById('targets-list');
const chartTitleEl = document.getElementById('chart-title');
const ctx = document.getElementById('latencyChart').getContext('2d');

let currentChart;

function colorForStatus(code) {
  if (code == null) return '#dc2626'; // treat timeouts as errors
  if (code >= 200 && code < 400) return '#16a34a';
  if (code >= 400 && code < 500) return '#f59e0b';
  return '#dc2626';
}

async function fetchJSON(url) {
  const res = await fetch(url, { headers: { 'Accept': 'application/json' } });
  if (!res.ok) throw new Error('Request failed: ' + res.status);
  return await res.json();
}

async function loadTargets() {
  try {
    const targets = await fetchJSON(`${API_BASE}/api/targets`);
    targetsListEl.innerHTML = '';
    targets.forEach(t => {
      const li = document.createElement('li');
      li.innerHTML = `<span class="badge">#${t.id}</span> <span>${t.url}</span>`;
      li.addEventListener('click', () => loadTargetStatus(t));
      targetsListEl.appendChild(li);
    });
  } catch (e) {
    targetsListEl.innerHTML = `<li style="color:#dc2626">Failed to load targets: ${e.message}</li>`;
  }
}

function renderChart(target, records) {
  const labels = records.map(r => new Date(r.checked_at).toLocaleTimeString());
  const data = records.map(r => r.response_time_ms || 0);
  const colors = records.map(r => colorForStatus(r.status_code));

  if (currentChart) currentChart.destroy();

  currentChart = new Chart(ctx, {
    type: 'line',
    data: {
      labels,
      datasets: [{
        label: 'Response time (ms)',
        data,
        borderColor: '#2563eb',
        backgroundColor: 'rgba(37, 99, 235, 0.15)',
        tension: 0.25,
        pointRadius: 4,
        pointHoverRadius: 6,
        pointBackgroundColor: colors,
        segment: {
          borderColor: ctx => colors[ctx.p0DataIndex]
        }
      }]
    },
    options: {
      responsive: true,
      scales: {
        y: { beginAtZero: true, title: { display: true, text: 'ms' } }
      },
      plugins: {
        legend: { display: true },
        tooltip: {
          callbacks: {
            label: (item) => {
              const status = records[item.dataIndex].status_code ?? 'timeout/error';
              return `Latency: ${item.formattedValue} ms (status: ${status})`;
            }
          }
        }
      }
    }
  });
}

async function loadTargetStatus(target) {
  chartTitleEl.textContent = `Metrics for ${target.url}`;
  try {
    const records = await fetchJSON(`${API_BASE}/api/status/${target.id}`);
    renderChart(target, records.reverse()); // draw oldest -> newest
  } catch (e) {
    chartTitleEl.textContent = `Failed to load metrics for ${target.url}: ${e.message}`;
  }
}

loadTargets();
// Poll the targets list occasionally to catch new additions
setInterval(loadTargets, 60_000);
