// ===== Tether Frontend Application =====
(function() {
    'use strict';

    // State
    let ws = null;
    let term = null;
    let fitAddon = null;
    let currentTerminalId = null;
    let terminals = [];
    let reconnectAttempts = 0;
    const MAX_RECONNECT_ATTEMPTS = 10;
    const RECONNECT_DELAY = 2000;

    // DOM Elements
    const elements = {
        menuBtn: document.getElementById('menu-btn'),
        drawer: document.getElementById('drawer'),
        drawerOverlay: document.getElementById('drawer-overlay'),
        drawerClose: document.getElementById('drawer-close'),
        newTerminalBtn: document.getElementById('new-terminal-btn'),
        terminalList: document.getElementById('terminal-list'),
        terminalContainer: document.getElementById('terminal-container'),
        terminal: document.getElementById('terminal'),
        presetBar: document.getElementById('preset-bar'),
        cmdInput: document.getElementById('cmd-input'),
        sendBtn: document.getElementById('send-btn'),
        statusIndicator: document.getElementById('status-indicator'),
        notificationBanner: document.getElementById('notification-banner'),
        dismissNotification: document.getElementById('dismiss-notification'),
    };

    // ===== Initialize xterm =====
    function initTerminal() {
        const isMobile = window.innerWidth <= 480;
        
        term = new Terminal({
            cursorBlink: true,
            fontSize: isMobile ? 12 : 14,
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

        fitAddon = new FitAddon.FitAddon();
        term.loadAddon(fitAddon);
        
        // Try WebGL addon for better performance
        try {
            const webglAddon = new WebglAddon.WebglAddon();
            term.loadAddon(webglAddon);
            webglAddon.onContextLoss(() => {
                webglAddon.dispose();
            });
        } catch (e) {
            console.log('WebGL not available, using canvas fallback');
        }

        term.open(elements.terminal);
        
        // Handle terminal input
        term.onData(data => {
            if (ws && ws.readyState === WebSocket.OPEN) {
                ws.send(data);
            }
        });

        // Handle resize
        term.onResize(size => {
            if (ws && ws.readyState === WebSocket.OPEN) {
                ws.send(JSON.stringify({
                    type: 'resize',
                    cols: size.cols,
                    rows: size.rows,
                }));
            }
        });

        // Initial fit
        setTimeout(() => fitAddon.fit(), 100);
        
        // Fit on resize
        window.addEventListener('resize', () => {
            if (fitAddon) fitAddon.fit();
        });

        // Fix iOS keyboard issues
        const textarea = term.textarea;
        if (textarea) {
            textarea.style.position = 'fixed';
            textarea.style.zIndex = '1000';
        }
    }

    // ===== WebSocket Connection =====
    function connectWebSocket(terminalId = null) {
        const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
        let url = `${protocol}//${window.location.host}/ws`;
        if (terminalId) {
            url += `?terminal_id=${encodeURIComponent(terminalId)}`;
        }

        if (ws) {
            ws.close();
        }

        ws = new WebSocket(url);

        ws.onopen = () => {
            console.log('WebSocket connected');
            elements.statusIndicator.classList.remove('disconnected');
            elements.statusIndicator.classList.add('connected');
            reconnectAttempts = 0;
            
            if (term) {
                term.focus();
            }
        };

        ws.onmessage = (event) => {
            if (term && event.data) {
                term.write(event.data);
            }
        };

        ws.onclose = () => {
            console.log('WebSocket disconnected');
            elements.statusIndicator.classList.remove('connected');
            elements.statusIndicator.classList.add('disconnected');
            
            // Attempt reconnect
            if (reconnectAttempts < MAX_RECONNECT_ATTEMPTS) {
                reconnectAttempts++;
                setTimeout(() => {
                    connectWebSocket(currentTerminalId);
                }, RECONNECT_DELAY);
            }
        };

        ws.onerror = (error) => {
            console.error('WebSocket error:', error);
        };
    }

    // ===== Terminal Management =====
    async function createTerminal() {
        try {
            const response = await fetch('/api/terminals/new', { method: 'POST' });
            const data = await response.json();
            if (data.id) {
                currentTerminalId = data.id;
                connectWebSocket(data.id);
                updateTerminalList();
                closeDrawer();
            }
        } catch (e) {
            console.error('Failed to create terminal:', e);
        }
    }

    async function switchTerminal(terminalId) {
        currentTerminalId = terminalId;
        connectWebSocket(terminalId);
        closeDrawer();
        
        // Clear terminal view
        if (term) {
            term.clear();
        }
    }

    async function closeTerminal(terminalId, event) {
        event.stopPropagation();
        try {
            await fetch(`/api/terminals/${terminalId}`, { method: 'DELETE' });
            terminals = terminals.filter(t => t.id !== terminalId);
            
            if (currentTerminalId === terminalId) {
                if (terminals.length > 0) {
                    switchTerminal(terminals[0].id);
                } else {
                    createTerminal();
                }
            }
            updateTerminalList();
        } catch (e) {
            console.error('Failed to close terminal:', e);
        }
    }

    async function updateTerminalList() {
        try {
            const response = await fetch('/api/terminals');
            const data = await response.json();
            terminals = data.terminals || [];
            renderTerminalList();
        } catch (e) {
            console.error('Failed to update terminal list:', e);
        }
    }

    function renderTerminalList() {
        elements.terminalList.innerHTML = '';
        
        terminals.forEach(t => {
            const item = document.createElement('div');
            item.className = `terminal-item${t.id === currentTerminalId ? ' active' : ''}`;
            item.innerHTML = `
                <span class="term-id">${t.id.substring(0, 8)}...</span>
                <span class="term-status${t.waiting_for_input ? ' waiting' : ''}">
                    ${t.waiting_for_input ? '⏳ Waiting' : '● Active'}
                </span>
                <button class="close-btn" data-id="${t.id}">✕</button>
            `;
            
            item.addEventListener('click', () => switchTerminal(t.id));
            
            const closeBtn = item.querySelector('.close-btn');
            closeBtn.addEventListener('click', (e) => closeTerminal(t.id, e));
            
            elements.terminalList.appendChild(item);
        });
    }

    // ===== Drawer =====
    function openDrawer() {
        elements.drawer.classList.add('open');
        elements.drawerOverlay.classList.add('active');
        updateTerminalList();
    }

    function closeDrawer() {
        elements.drawer.classList.remove('open');
        elements.drawerOverlay.classList.remove('active');
    }

    // ===== Preset Buttons =====
    function setupPresetButtons() {
        document.querySelectorAll('.preset-btn').forEach(btn => {
            btn.addEventListener('click', () => {
                const input = btn.dataset.input;
                if (ws && ws.readyState === WebSocket.OPEN) {
                    ws.send(input);
                }
            });
        });
    }

    // ===== Custom Input =====
    function setupCustomInput() {
        elements.sendBtn.addEventListener('click', sendCustomCommand);
        
        elements.cmdInput.addEventListener('keydown', (e) => {
            if (e.key === 'Enter') {
                sendCustomCommand();
            }
        });
    }

    function sendCustomCommand() {
        const cmd = elements.cmdInput.value;
        if (!cmd) return;
        
        if (ws && ws.readyState === WebSocket.OPEN) {
            ws.send(cmd + '\n');
        }
        
        elements.cmdInput.value = '';
        elements.cmdInput.blur();
    }

    // ===== Notifications =====
    function showNotification() {
        elements.notificationBanner.classList.remove('hidden');
        
        // Vibrate if available
        if (navigator.vibrate) {
            navigator.vibrate([200, 100, 200]);
        }
        
        // Request notification permission for push
        if ('Notification' in window && Notification.permission === 'default') {
            Notification.requestPermission();
        }
    }

    function hideNotification() {
        elements.notificationBanner.classList.add('hidden');
    }

    // ===== Touch Gestures for Drawer =====
    function setupTouchGestures() {
        let startX = 0;
        let currentX = 0;
        let isDragging = false;

        document.addEventListener('touchstart', (e) => {
            startX = e.touches[0].clientX;
            // Only allow swipe from right edge
            if (startX > window.innerWidth - 30) {
                isDragging = true;
            }
        });

        document.addEventListener('touchmove', (e) => {
            if (!isDragging) return;
            currentX = e.touches[0].clientX;
        });

        document.addEventListener('touchend', () => {
            if (!isDragging) return;
            isDragging = false;
            
            const diff = startX - currentX;
            // Swipe left to open
            if (diff > 50 && !elements.drawer.classList.contains('open')) {
                openDrawer();
            }
            // Swipe right to close
            if (diff < -50 && elements.drawer.classList.contains('open')) {
                closeDrawer();
            }
        });
    }

    // ===== Keyboard Handling for Mobile =====
    function setupKeyboardHandling() {
        const textarea = term?.textarea;
        if (textarea) {
            textarea.addEventListener('focus', () => {
                setTimeout(() => fitAddon.fit(), 300);
            });
            
            textarea.addEventListener('blur', () => {
                setTimeout(() => fitAddon.fit(), 100);
            });
        }
    }

    // ===== Periodic Updates =====
    function startPeriodicUpdates() {
        setInterval(() => {
            updateTerminalList();
            
            // Check if any terminal is waiting for input
            const waitingTerminal = terminals.find(t => t.waiting_for_input);
            if (waitingTerminal && waitingTerminal.id !== currentTerminalId) {
                showNotification();
            }
        }, 5000);
    }

    // ===== Initialize =====
    function init() {
        initTerminal();
        setupPresetButtons();
        setupCustomInput();
        setupTouchGestures();
        setupKeyboardHandling();
        
        // Event listeners
        elements.menuBtn.addEventListener('click', openDrawer);
        elements.drawerClose.addEventListener('click', closeDrawer);
        elements.drawerOverlay.addEventListener('click', closeDrawer);
        elements.newTerminalBtn.addEventListener('click', createTerminal);
        elements.dismissNotification.addEventListener('click', hideNotification);
        
        // Create initial terminal
        createTerminal();
        
        // Start periodic updates
        startPeriodicUpdates();
        
        // Request notification permission
        if ('Notification' in window && Notification.permission === 'default') {
            // Will request on first waiting event
        }
    }

    // Start when DOM is ready
    if (document.readyState === 'loading') {
        document.addEventListener('DOMContentLoaded', init);
    } else {
        init();
    }
})();
