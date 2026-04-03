// ===== Tether Frontend - Terminal Mirroring =====
(function() {
    'use strict';

    const terminals = new Map();
    const mirrors = new Map();
    let activeId = null;
    let reconnectAttempts = {};
    const MAX_RECONNECT = 10;
    const RECONNECT_DELAY = 2000;

    const els = {
        header: document.getElementById('header'),
        tabs: document.getElementById('terminal-tabs'),
        views: document.getElementById('terminal-views'),
        emptyState: document.getElementById('empty-state'),
        newBtn: document.getElementById('new-terminal-btn'),
        refreshBtn: document.getElementById('refresh-btn'),
        emptyNewBtn: document.getElementById('empty-new-btn'),
        notificationBanner: document.getElementById('notification-banner'),
        dismissNotification: document.getElementById('dismiss-notification'),
    };

    // ===== Create Terminal Panel =====
    function createPanel(id, label, type) {
        const tabBtn = document.createElement('button');
        tabBtn.className = 'tab-btn';
        tabBtn.dataset.id = id;
        tabBtn.dataset.type = type;
        tabBtn.innerHTML = `<span class="tab-status"></span><span class="tab-label">${label}</span><span class="tab-close" data-id="${id}">✕</span>`;
        tabBtn.addEventListener('click', (e) => { if (!e.target.classList.contains('tab-close')) activatePanel(id); });
        tabBtn.querySelector('.tab-close').addEventListener('click', (e) => { e.stopPropagation(); removePanel(id); });
        els.tabs.appendChild(tabBtn);

        const panel = document.createElement('div');
        panel.className = 'terminal-panel';
        panel.dataset.id = id;
        panel.innerHTML = `
            <div class="terminal-panel-header">
                <span class="panel-title">${label}</span>
                <span class="panel-status">connecting...</span>
            </div>
            <div class="xterm-wrapper"><div class="xterm-container"></div></div>`;
        els.views.appendChild(panel);

        const isMobile = window.innerWidth <= 480;
        const term = new Terminal({
            cursorBlink: true, fontSize: isMobile ? 11 : 13,
            fontFamily: "'SF Mono', 'Fira Code', 'Cascadia Code', 'Menlo', monospace",
            theme: { background: '#000000', foreground: '#ffffff', cursor: '#00d4ff', selectionBackground: '#00d4ff40',
                black: '#000000', red: '#e94560', green: '#00ff88', yellow: '#ffbd2e', blue: '#00d4ff', magenta: '#bd93f9', cyan: '#8be9fd', white: '#ffffff' },
            allowProposedApi: true, scrollback: 10000,
        });
        const fitAddon = new FitAddon.FitAddon();
        term.loadAddon(fitAddon);
        try { const wg = new WebglAddon.WebglAddon(); term.loadAddon(wg); wg.onContextLoss(() => wg.dispose()); } catch(e) {}
        term.open(panel.querySelector('.xterm-container'));

        term.onData(data => {
            const t = terminals.get(id) || mirrors.get(id);
            if (t && t.ws && t.ws.readyState === WebSocket.OPEN) t.ws.send(data);
        });
        term.onResize(size => {
            const t = terminals.get(id) || mirrors.get(id);
            if (t && t.ws && t.ws.readyState === WebSocket.OPEN) t.ws.send(JSON.stringify({ type: 'resize', cols: size.cols, rows: size.rows }));
        });
        setTimeout(() => { fitAddon.fit(); term.focus(); }, 100);

        const entry = { ws: null, term, fitAddon, panel, tabBtn, type };
        if (type === 'mirror') mirrors.set(id, entry);
        else terminals.set(id, entry);
        reconnectAttempts[id] = 0;
        updateEmptyState();
        return id;
    }

    // ===== WebSocket =====
    function connectWS(id, param, value) {
        const t = terminals.get(id) || mirrors.get(id);
        if (!t) return;
        if (t.ws) t.ws.close();

        const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
        const url = `${protocol}//${window.location.host}/ws?${param}=${encodeURIComponent(value)}`;
        t.ws = new WebSocket(url);

        t.ws.onopen = () => { reconnectAttempts[id] = 0; updateStatus(id, true); };
        t.ws.onmessage = (ev) => { if (ev.data && typeof ev.data === 'string') t.term.write(ev.data); };
        t.ws.onclose = () => {
            updateStatus(id, false);
            if (reconnectAttempts[id] < MAX_RECONNECT) {
                reconnectAttempts[id]++;
                setTimeout(() => connectWS(id, param, value), RECONNECT_DELAY);
            }
        };
    }

    function updateStatus(id, connected) {
        const t = terminals.get(id) || mirrors.get(id);
        if (!t) return;
        const status = t.tabBtn.querySelector('.tab-status');
        if (status) status.className = 'tab-status' + (connected ? '' : ' disconnected');
        const ps = t.panel.querySelector('.panel-status');
        if (ps) { ps.textContent = connected ? '● connected' : '○ disconnected'; ps.className = 'panel-status' + (connected ? ' connected' : ''); }
    }

    // ===== Panel Management =====
    function activatePanel(id) {
        terminals.forEach((t, tid) => { t.tabBtn.classList.remove('active'); t.panel.style.display = 'none'; });
        mirrors.forEach((t, tid) => { t.tabBtn.classList.remove('active'); t.panel.style.display = 'none'; });
        const t = terminals.get(id) || mirrors.get(id);
        if (!t) return;
        t.tabBtn.classList.add('active');
        t.panel.style.display = 'flex';
        activeId = id;
        setTimeout(() => t.fitAddon.fit(), 50);
    }

    function removePanel(id) {
        const t = terminals.get(id) || mirrors.get(id);
        if (!t) return;
        if (t.ws) t.ws.close();
        t.tabBtn.remove(); t.panel.remove(); t.term.dispose();
        terminals.delete(id); mirrors.delete(id); delete reconnectAttempts[id];
        if (activeId === id) {
            activeId = null;
            const next = terminals.keys().next().value || mirrors.keys().next().value;
            if (next) activatePanel(next);
        }
        updateEmptyState();
    }

    function updateEmptyState() {
        const has = terminals.size > 0 || mirrors.size > 0;
        els.emptyState.classList.toggle('hidden', has);
        els.tabs.style.display = has ? 'flex' : 'none';
        els.views.style.display = has ? 'block' : 'none';
    }

    // ===== Mirror Discovery & Setup =====
    async function discoverMirrors() {
        try {
            const res = await fetch('/api/mirror/discover');
            const data = await res.json();
            return data.terminals || [];
        } catch(e) { return []; }
    }

    async function setupAllMirrors() {
        try {
            const res = await fetch('/api/mirror/setup-all', { method: 'POST' });
            const data = await res.json();
            return data.results || [];
        } catch(e) { return []; }
    }

    function showSetupUI(terminals) {
        // Show a modal with setup commands for each terminal
        const existing = document.getElementById('mirror-setup-modal');
        if (existing) existing.remove();

        const modal = document.createElement('div');
        modal.id = 'mirror-setup-modal';
        modal.style.cssText = 'position:fixed;top:0;left:0;right:0;bottom:0;background:rgba(0,0,0,0.8);z-index:1000;display:flex;align-items:center;justify-content:center;padding:1rem;';
        modal.innerHTML = `
            <div style="background:#16213e;border-radius:12px;max-width:700px;width:100%;max-height:80vh;overflow-y:auto;padding:1.5rem;">
                <div style="display:flex;justify-content:space-between;align-items:center;margin-bottom:1rem;">
                    <h2 style="color:#00d4ff;margin:0;font-size:1.2rem;">🪢 Setup Terminal Mirrors</h2>
                    <button id="close-setup" style="background:none;border:none;color:#fff;font-size:1.5rem;cursor:pointer;">✕</button>
                </div>
                <p style="color:#aaa;margin-bottom:1rem;font-size:0.85rem;">Run these commands in each terminal tab to enable mirroring:</p>
                <div id="setup-commands"></div>
                <button id="setup-all-btn" style="background:#00d4ff;color:#000;border:none;padding:0.75rem 1.5rem;border-radius:8px;font-weight:600;cursor:pointer;margin-top:1rem;width:100%;">
                    Start All Mirrors
                </button>
            </div>`;
        document.body.appendChild(modal);

        const container = modal.querySelector('#setup-commands');
        terminals.forEach(t => {
            const div = document.createElement('div');
            div.style.cssText = 'background:#0d1117;border-radius:8px;padding:0.75rem;margin-bottom:0.5rem;';
            div.innerHTML = `
                <div style="display:flex;justify-content:space-between;align-items:center;">
                    <span style="color:#00ff88;font-weight:600;">${t.id}</span>
                    <span style="color:#888;font-size:0.75rem;">${t.pts_path} (PID ${t.pid})</span>
                </div>
                <div style="display:flex;gap:0.5rem;margin-top:0.5rem;">
                    <code style="flex:1;background:#161b22;padding:0.5rem;border-radius:4px;font-size:0.8rem;color:#fff;overflow-x:auto;white-space:nowrap;">${t.setup_command}</code>
                    <button class="copy-cmd" data-cmd="${t.setup_command}" style="background:#238636;color:#fff;border:none;padding:0.5rem 0.75rem;border-radius:4px;cursor:pointer;font-size:0.75rem;white-space:nowrap;">Copy</button>
                </div>`;
            container.appendChild(div);
        });

        modal.querySelectorAll('.copy-cmd').forEach(btn => {
            btn.addEventListener('click', () => {
                navigator.clipboard.writeText(btn.dataset.cmd);
                btn.textContent = '✓ Copied';
                setTimeout(() => btn.textContent = 'Copy', 2000);
            });
        });

        modal.querySelector('#close-setup').addEventListener('click', () => modal.remove());
        modal.querySelector('#setup-all-btn').addEventListener('click', async () => {
            const results = await setupAllMirrors();
            modal.remove();
            connectMirrors(results);
        });
    }

    function connectMirrors(results) {
        results.forEach(r => {
            if (r.status === 'ok') {
                const id = r.id;
                if (!mirrors.has(id)) {
                    createPanel(id, `Mirror: ${id}`, 'mirror');
                    connectWS(id, 'mirror_id', id);
                }
            }
        });
        if (mirrors.size > 0) activatePanel(mirrors.keys().next().value);
    }

    // ===== New Terminal =====
    async function createNewTerminal() {
        try {
            const res = await fetch('/api/terminals/new', { method: 'POST' });
            const data = await res.json();
            if (data.id) {
                createPanel(data.id, `Terminal ${data.id.substring(0,8)}`, 'pty');
                connectWS(data.id, 'terminal_id', data.id);
                activatePanel(data.id);
            }
        } catch(e) { console.error('Failed to create terminal:', e); }
    }

    // ===== Refresh =====
    async function refreshTerminals() {
        const discovered = await discoverMirrors();
        if (discovered.length > 0 && mirrors.size === 0) {
            showSetupUI(discovered);
        }
    }

    // ===== Init =====
    function init() {
        els.newBtn.addEventListener('click', createNewTerminal);
        els.emptyNewBtn.addEventListener('click', createNewTerminal);
        els.refreshBtn.addEventListener('click', refreshTerminals);
        els.dismissNotification.addEventListener('click', () => els.notificationBanner.classList.add('hidden'));
        refreshTerminals();
        setInterval(refreshTerminals, 15000);
    }

    if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', init);
    else init();
})();
