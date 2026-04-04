// ===== Tether - Mobile Web Terminal =====
(function() {
    'use strict';

    const terminals = new Map();
    let activeId = null;
    let reconnectAttempts = {};
    const MAX_RECONNECT = 10;
    const RECONNECT_DELAY = 2000;
    let terminalCounter = 0;

    const els = {
        header: document.getElementById('header'),
        tabs: document.getElementById('terminal-tabs'),
        views: document.getElementById('terminal-views'),
        emptyState: document.getElementById('empty-state'),
        newBtn: document.getElementById('new-terminal-btn'),
        refreshBtn: document.getElementById('refresh-btn'),
        emptyNewBtn: document.getElementById('empty-new-btn'),
    };

    // ===== Create Terminal Panel =====
    function createPanel(id, label, isMirror) {
        terminalCounter++;

        // Tab button
        const tabBtn = document.createElement('button');
        tabBtn.className = 'tab-btn';
        tabBtn.dataset.id = id;
        tabBtn.innerHTML = `
            <span class="tab-status"></span>
            <span class="tab-label">${label}</span>
            <span class="tab-close" data-id="${id}">✕</span>`;
        tabBtn.addEventListener('click', (e) => {
            if (!e.target.classList.contains('tab-close')) activatePanel(id);
        });
        tabBtn.querySelector('.tab-close').addEventListener('click', (e) => {
            e.stopPropagation();
            removePanel(id);
        });
        els.tabs.appendChild(tabBtn);

        // Panel view
        const panel = document.createElement('div');
        panel.className = 'terminal-panel';
        panel.dataset.id = id;
        panel.innerHTML = `
            <div class="terminal-panel-header">
                <span class="panel-title">${label}</span>
                <span class="panel-status">connecting...</span>
            </div>
            <div class="xterm-wrapper">
                <div class="xterm-container"></div>
            </div>`;
        els.views.appendChild(panel);

        // xterm.js
        const isMobile = window.innerWidth <= 480;
        const term = new Terminal({
            cursorBlink: true,
            cursorStyle: 'block',
            fontSize: isMobile ? 12 : 13,
            fontFamily: "'SF Mono', 'Fira Code', 'Cascadia Code', 'Menlo', 'Consolas', monospace",
            theme: {
                background: '#0a0e14',
                foreground: '#b3b1ad',
                cursor: '#00d4ff',
                cursorAccent: '#0a0e14',
                selectionBackground: '#00d4ff30',
                black: '#1d2021',
                red: '#cc241d',
                green: '#98971a',
                yellow: '#d79921',
                blue: '#458588',
                magenta: '#b16286',
                cyan: '#689d6a',
                white: '#a89984',
                brightBlack: '#928374',
                brightRed: '#fb4934',
                brightGreen: '#b8bb26',
                brightYellow: '#fabd2f',
                brightBlue: '#83a598',
                brightMagenta: '#d3869b',
                brightCyan: '#8ec07c',
                brightWhite: '#ebdbb2',
            },
            allowProposedApi: true,
            scrollback: 10000,
            convertEol: true,
        });

        const fitAddon = new FitAddon.FitAddon();
        term.loadAddon(fitAddon);

        try {
            const wg = new WebglAddon.WebglAddon();
            term.loadAddon(wg);
            wg.onContextLoss(() => wg.dispose());
        } catch(e) { /* WebGL not available, fallback to canvas */ }

        term.open(panel.querySelector('.xterm-container'));

        // Send input to server
        term.onData(data => {
            const t = terminals.get(id);
            if (t && t.ws && t.ws.readyState === WebSocket.OPEN) {
                t.ws.send(data);
            }
        });

        // Handle resize
        term.onResize(size => {
            const t = terminals.get(id);
            if (t && t.ws && t.ws.readyState === WebSocket.OPEN) {
                t.ws.send(JSON.stringify({ type: 'resize', cols: size.cols, rows: size.rows }));
            }
        });

        // Fit and focus - wait for container to have dimensions
        function waitForFit() {
            const container = panel.querySelector('.xterm-container');
            const viewsEl = document.getElementById('terminal-views');
            const headerEl = document.getElementById('header');
            const tabsEl = document.getElementById('terminal-tabs');
            const headerHeight = headerEl ? headerEl.getBoundingClientRect().height : 48;
            const tabsHeight = tabsEl ? tabsEl.getBoundingClientRect().height : 0;
            const availableHeight = window.innerHeight - headerHeight - tabsHeight;

            // Force container height
            container.style.height = Math.max(availableHeight, 200) + 'px';
            container.style.minHeight = Math.max(availableHeight, 200) + 'px';

            const rect = container.getBoundingClientRect();
            if (rect.width > 0 && rect.height > 0) {
                fitAddon.fit();
                term.focus();
            } else {
                setTimeout(waitForFit, 50);
            }
        }
        setTimeout(waitForFit, 50);

        // Store
        terminals.set(id, { ws: null, term, fitAddon, panel, tabBtn, isMirror });
        reconnectAttempts[id] = 0;
        updateEmptyState();
        return id;
    }

    // ===== WebSocket Connection =====
    function connectTerminal(id) {
        const t = terminals.get(id);
        if (!t) return;

        if (t.ws) t.ws.close();

        const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
        const wsParam = t.isMirror ? 'mirror_id' : 'terminal_id';
        const url = `${protocol}//${window.location.host}/ws?${wsParam}=${encodeURIComponent(id)}`;
        t.ws = new WebSocket(url);

        t.ws.onopen = () => {
            reconnectAttempts[id] = 0;
            updateStatus(id, true);
        };

        t.ws.onmessage = (ev) => {
            if (ev.data && typeof ev.data === 'string') {
                t.term.write(ev.data);
            }
        };

        t.ws.onclose = () => {
            updateStatus(id, false);
            if (reconnectAttempts[id] < MAX_RECONNECT) {
                reconnectAttempts[id]++;
                setTimeout(() => connectTerminal(id), RECONNECT_DELAY);
            }
        };

        t.ws.onerror = () => {
            updateStatus(id, false);
        };
    }

    function updateStatus(id, connected) {
        const t = terminals.get(id);
        if (!t) return;

        const status = t.tabBtn.querySelector('.tab-status');
        if (status) {
            status.className = 'tab-status' + (connected ? '' : ' disconnected');
        }

        const ps = t.panel.querySelector('.panel-status');
        if (ps) {
            ps.textContent = connected ? '● connected' : '○ disconnected';
            ps.className = 'panel-status' + (connected ? ' connected' : '');
        }
    }

    // ===== Panel Management =====
    function activatePanel(id) {
        terminals.forEach((t, tid) => {
            t.tabBtn.classList.remove('active');
            t.panel.style.display = 'none';
        });

        const t = terminals.get(id);
        if (!t) return;

        t.tabBtn.classList.add('active');
        t.panel.style.display = 'flex';
        activeId = id;
        setTimeout(() => t.fitAddon.fit(), 50);
        t.term.focus();
    }

    function removePanel(id) {
        const t = terminals.get(id);
        if (!t) return;

        if (t.ws) t.ws.close();
        t.tabBtn.remove();
        t.panel.remove();
        t.term.dispose();
        terminals.delete(id);
        delete reconnectAttempts[id];

        if (activeId === id) {
            activeId = null;
            const next = terminals.keys().next().value;
            if (next) activatePanel(next);
        }

        updateEmptyState();
    }

    function updateEmptyState() {
        const has = terminals.size > 0;
        els.emptyState.classList.toggle('hidden', has);
        els.tabs.style.display = has ? 'flex' : 'none';
        els.views.style.display = has ? 'block' : 'none';
    }

    // ===== New Terminal =====
    async function createNewTerminal() {
        try {
            const res = await fetch('/api/terminals/new', { method: 'POST' });
            const data = await res.json();
            if (data.id) {
                const label = `Terminal ${terminalCounter + 1}`;
                createPanel(data.id, label, false);
                connectTerminal(data.id);
                activatePanel(data.id);
            }
        } catch(e) {
            console.error('Failed to create terminal:', e);
        }
    }

    // ===== Discover & Mirror Laptop Terminals =====
    async function discoverAndMirror() {
        try {
            const res = await fetch('/api/mirror/discover');
            const data = await res.json();
            if (!data.terminals || data.terminals.length === 0) {
                console.log('No laptop terminals discovered');
                return false;
            }

            // Setup all mirrors
            const setupRes = await fetch('/api/mirror/setup-all', { method: 'POST' });
            const setupData = await setupRes.json();
            console.log('Mirror setup:', setupData);

            // Create panels for each discovered terminal
            let firstId = null;
            for (const t of data.terminals) {
                const label = `${t.command} (pts/${t.pts_path.split('/').pop()})`;
                const panelId = createPanel(t.id, label, true);
                connectTerminal(t.id);
                if (!firstId) firstId = t.id;
            }

            if (firstId) activatePanel(firstId);
            return true;
        } catch(e) {
            console.log('Could not discover/mirror laptop terminals:', e);
            return false;
        }
    }

    // ===== Refresh (reconnect all) =====
    function refreshTerminals() {
        terminals.forEach((t, id) => {
            if (!t.ws || t.ws.readyState !== WebSocket.OPEN) {
                connectTerminal(id);
            }
        });
    }

    // ===== Init =====
    async function init() {
        els.newBtn.addEventListener('click', createNewTerminal);
        els.emptyNewBtn.addEventListener('click', createNewTerminal);
        els.refreshBtn.addEventListener('click', refreshTerminals);

        // Try to discover and mirror laptop terminals first
        const mirrored = await discoverAndMirror();

        // If no laptop terminals found, create a fresh PTY terminal
        if (!mirrored) {
            createNewTerminal();
        }

        // Auto-reconnect every 15s
        setInterval(refreshTerminals, 15000);
    }

    if (document.readyState === 'loading') {
        document.addEventListener('DOMContentLoaded', init);
    } else {
        init();
    }
})();
