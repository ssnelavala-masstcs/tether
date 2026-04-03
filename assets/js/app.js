// ===== Tether Frontend - Multi-Terminal + tmux Mirroring =====
(function() {
    'use strict';

    // State
    const terminals = new Map(); // terminalId -> { ws, term, fitAddon, el, type: 'pty'|'tmux' }
    let activeTerminalId = null;
    let reconnectAttempts = {};
    const MAX_RECONNECT_ATTEMPTS = 10;
    const RECONNECT_DELAY = 2000;
    let tmuxAvailable = false;

    // DOM Elements
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

    // ===== Terminal Management =====
    function createTerminalElement(id, type = 'pty', tmuxPane = null) {
        const label = type === 'tmux'
            ? tmuxPane.session_name + ':' + tmuxPane.pane_index
            : id.substring(0, 8);

        // Create tab button
        const tabBtn = document.createElement('button');
        tabBtn.className = 'tab-btn';
        tabBtn.dataset.id = id;
        tabBtn.dataset.type = type;
        tabBtn.innerHTML = `
            <span class="tab-status"></span>
            <span class="tab-label">${label}</span>
            <span class="tab-close" data-id="${id}">✕</span>
        `;
        tabBtn.addEventListener('click', (e) => {
            if (!e.target.classList.contains('tab-close')) {
                activateTerminal(id);
            }
        });
        tabBtn.querySelector('.tab-close').addEventListener('click', (e) => {
            e.stopPropagation();
            removeTerminal(id);
        });
        els.tabs.appendChild(tabBtn);

        // Create terminal panel
        const panel = document.createElement('div');
        panel.className = 'terminal-panel';
        panel.dataset.id = id;
        panel.dataset.type = type;
        const headerLabel = type === 'tmux'
            ? `tmux: ${tmuxPane.session_name}:${tmuxPane.pane_index}`
            : `Terminal ${id.substring(0, 8)}`;
        panel.innerHTML = `
            <div class="terminal-panel-header">
                <span class="panel-title">${headerLabel}</span>
                <span class="panel-status">connecting...</span>
            </div>
            <div class="xterm-wrapper">
                <div class="xterm-container"></div>
            </div>
        `;
        els.views.appendChild(panel);

        // Create xterm instance
        const isMobile = window.innerWidth <= 480;
        const term = new Terminal({
            cursorBlink: true,
            fontSize: isMobile ? 11 : 13,
            fontFamily: "'SF Mono', 'Fira Code', 'Cascadia Code', 'Menlo', monospace",
            theme: {
                background: '#000000',
                foreground: '#ffffff',
                cursor: '#00d4ff',
                selectionBackground: '#00d4ff40',
                black: '#000000',
                red: '#e94560',
                green: '#00ff88',
                yellow: '#ffbd2e',
                blue: '#00d4ff',
                magenta: '#bd93f9',
                cyan: '#8be9fd',
                white: '#ffffff',
            },
            allowProposedApi: true,
            scrollback: 10000,
        });

        const fitAddon = new FitAddon.FitAddon();
        term.loadAddon(fitAddon);

        try {
            const webglAddon = new WebglAddon.WebglAddon();
            term.loadAddon(webglAddon);
            webglAddon.onContextLoss(() => webglAddon.dispose());
        } catch (e) {
            // WebGL not available
        }

        const container = panel.querySelector('.xterm-container');
        term.open(container);

        // Handle terminal input
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

        // Fit after a short delay
        setTimeout(() => {
            fitAddon.fit();
            term.focus();
        }, 100);

        // Store terminal data
        terminals.set(id, { ws: null, term, fitAddon, panel, tabBtn, type, tmuxPane });
        reconnectAttempts[id] = 0;

        updateEmptyState();
        return id;
    }

    // ===== WebSocket Connection =====
    function connectWebSocket(terminalId, type = 'pty', tmuxPane = null) {
        const t = terminals.get(terminalId);
        if (!t) return;

        // Close existing connection
        if (t.ws) {
            t.ws.close();
        }

        const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
        let url;
        if (type === 'tmux' && tmuxPane) {
            url = `${protocol}//${window.location.host}/ws?tmux_pane=${encodeURIComponent(tmuxPane.pane_id)}`;
        } else {
            url = `${protocol}//${window.location.host}/ws?terminal_id=${encodeURIComponent(terminalId)}`;
        }

        t.ws = new WebSocket(url);

        t.ws.onopen = () => {
            reconnectAttempts[terminalId] = 0;
            updateTabStatus(terminalId, true);
            updatePanelStatus(terminalId, true);
        };

        t.ws.onmessage = (event) => {
            if (event.data && typeof event.data === 'string') {
                t.term.write(event.data);
            }
        };

        t.ws.onclose = () => {
            updateTabStatus(terminalId, false);
            updatePanelStatus(terminalId, false);

            // Attempt reconnect
            if (reconnectAttempts[terminalId] < MAX_RECONNECT_ATTEMPTS) {
                reconnectAttempts[terminalId]++;
                setTimeout(() => connectWebSocket(terminalId, type, tmuxPane), RECONNECT_DELAY);
            }
        };

        t.ws.onerror = () => {
            updateTabStatus(terminalId, false);
        };
    }

    function updateTabStatus(id, connected) {
        const t = terminals.get(id);
        if (!t) return;
        const status = t.tabBtn.querySelector('.tab-status');
        if (status) {
            status.className = 'tab-status' + (connected ? '' : ' disconnected');
        }
    }

    function updatePanelStatus(id, connected) {
        const t = terminals.get(id);
        if (!t) return;
        const status = t.panel.querySelector('.panel-status');
        if (status) {
            status.textContent = connected ? '● connected' : '○ disconnected';
            status.className = 'panel-status' + (connected ? ' connected' : '');
        }
    }

    // ===== Terminal Activation =====
    function activateTerminal(id) {
        if (!terminals.has(id)) return;

        // Deactivate all
        terminals.forEach((t, tid) => {
            t.tabBtn.classList.remove('active');
            t.panel.style.display = 'none';
        });

        // Activate selected
        const t = terminals.get(id);
        t.tabBtn.classList.add('active');
        t.panel.style.display = 'flex';
        activeTerminalId = id;

        // Refit terminal
        setTimeout(() => t.fitAddon.fit(), 50);
    }

    // ===== Terminal Removal =====
    function removeTerminal(id) {
        const t = terminals.get(id);
        if (!t) return;

        // Close WebSocket
        if (t.ws) t.ws.close();

        // For PTY terminals, delete from server
        if (t.type === 'pty') {
            deleteTerminal(id);
        }

        // Remove DOM elements
        t.tabBtn.remove();
        t.panel.remove();

        // Dispose xterm
        t.term.dispose();

        terminals.delete(id);
        delete reconnectAttempts[id];

        // If we removed the active terminal, activate another
        if (activeTerminalId === id) {
            activeTerminalId = null;
            const nextId = terminals.keys().next().value;
            if (nextId) {
                activateTerminal(nextId);
            }
        }

        updateEmptyState();
    }

    function updateEmptyState() {
        if (terminals.size === 0) {
            els.emptyState.classList.remove('hidden');
            els.tabs.style.display = 'none';
            els.views.style.display = 'none';
        } else {
            els.emptyState.classList.add('hidden');
            els.tabs.style.display = 'flex';
            els.views.style.display = 'block';
        }
    }

    // ===== API Calls =====
    async function createNewTerminal() {
        try {
            const response = await fetch('/api/terminals/new', { method: 'POST' });
            const data = await response.json();
            if (data.id) {
                createTerminalElement(data.id, 'pty');
                connectWebSocket(data.id, 'pty');
                activateTerminal(data.id);
            }
        } catch (e) {
            console.error('Failed to create terminal:', e);
        }
    }

    async function deleteTerminal(id) {
        try {
            await fetch(`/api/terminals/${id}`, { method: 'DELETE' });
        } catch (e) {
            console.error('Failed to delete terminal:', e);
        }
    }

    async function loadTmuxSessions() {
        try {
            const response = await fetch('/api/tmux/sessions');
            if (!response.ok) {
                tmuxAvailable = false;
                return [];
            }
            const data = await response.json();
            tmuxAvailable = true;
            return data.panes || [];
        } catch (e) {
            tmuxAvailable = false;
            return [];
        }
    }

    async function loadExistingTerminals() {
        try {
            const response = await fetch('/api/terminals');
            const data = await response.json();
            const serverTerminals = data.terminals || [];

            // Also check for tmux sessions
            const tmuxPanes = await loadTmuxSessions();

            let createdAny = false;

            // Create UI for tmux panes (mirrored sessions)
            tmuxPanes.forEach(pane => {
                const id = 'tmux-' + pane.pane_id.replace(/[^a-zA-Z0-9]/g, '_');
                if (!terminals.has(id)) {
                    createTerminalElement(id, 'tmux', pane);
                    connectWebSocket(id, 'tmux', pane);
                    createdAny = true;
                }
            });

            // Create UI for server terminals
            serverTerminals.forEach(t => {
                if (!terminals.has(t.id)) {
                    createTerminalElement(t.id, 'pty');
                    connectWebSocket(t.id, 'pty');
                    createdAny = true;
                }
            });

            // If nothing exists, create a new terminal or show tmux message
            if (!createdAny) {
                if (tmuxPanes.length === 0 && serverTerminals.length === 0) {
                    // Show empty state with tmux hint
                    updateEmptyState();
                    if (tmuxAvailable) {
                        // tmux is available but no sessions - create a new terminal
                        createNewTerminal();
                    } else {
                        // No tmux, no terminals - create one
                        createNewTerminal();
                    }
                    return;
                }
            }

            // Activate the first one
            const firstId = terminals.keys().next().value;
            if (firstId) {
                activateTerminal(firstId);
            }
        } catch (e) {
            console.error('Failed to load terminals:', e);
            createNewTerminal();
        }
    }

    // ===== Refresh =====
    async function refreshTerminals() {
        try {
            const response = await fetch('/api/terminals');
            const data = await response.json();
            const serverTerminals = data.terminals || [];

            // Check tmux sessions
            const tmuxPanes = await loadTmuxSessions();
            const tmuxIds = new Set(tmuxPanes.map(p => 'tmux-' + p.pane_id.replace(/[^a-zA-Z0-9]/g, '_')));
            const serverIds = new Set(serverTerminals.map(t => t.id));

            // Remove PTY terminals that no longer exist on server
            const localIds = Array.from(terminals.keys());
            localIds.forEach(id => {
                const t = terminals.get(id);
                if (t && t.type === 'pty' && !serverIds.has(id)) {
                    removeTerminal(id);
                }
            });

            // Add new PTY terminals from server
            serverTerminals.forEach(t => {
                if (!terminals.has(t.id)) {
                    createTerminalElement(t.id, 'pty');
                    connectWebSocket(t.id, 'pty');
                }
            });

            // Update tmux panes - remove ones that disappeared, add new ones
            tmuxPanes.forEach(pane => {
                const id = 'tmux-' + pane.pane_id.replace(/[^a-zA-Z0-9]/g, '_');
                if (!terminals.has(id)) {
                    createTerminalElement(id, 'tmux', pane);
                    connectWebSocket(id, 'tmux', pane);
                }
            });

            // Ensure one is active
            if (!activeTerminalId && terminals.size > 0) {
                activateTerminal(terminals.keys().next().value);
            }
        } catch (e) {
            console.error('Failed to refresh terminals:', e);
        }
    }

    // ===== Notifications =====
    function showNotification() {
        els.notificationBanner.classList.remove('hidden');
        if (navigator.vibrate) navigator.vibrate([200, 100, 200]);
    }

    function hideNotification() {
        els.notificationBanner.classList.add('hidden');
    }

    // ===== Window Resize =====
    window.addEventListener('resize', () => {
        terminals.forEach(t => {
            setTimeout(() => t.fitAddon.fit(), 100);
        });
    });

    // ===== Periodic Updates =====
    function startPeriodicUpdates() {
        setInterval(() => {
            refreshTerminals();
        }, 10000);
    }

    // ===== Initialize =====
    function init() {
        // Event listeners
        els.newBtn.addEventListener('click', createNewTerminal);
        els.emptyNewBtn.addEventListener('click', createNewTerminal);
        els.refreshBtn.addEventListener('click', refreshTerminals);
        els.dismissNotification.addEventListener('click', hideNotification);

        // Load existing terminals from server (PTY + tmux)
        loadExistingTerminals();

        // Start periodic updates
        startPeriodicUpdates();
    }

    if (document.readyState === 'loading') {
        document.addEventListener('DOMContentLoaded', init);
    } else {
        init();
    }
})();
