const logContainer = document.getElementById('log-area');

const ansiRegex = /\x1b\[([0-9;]+)m/g;

const ansiColorMap = {
    '30': 'ansi-black',
    '31': 'ansi-red',
    '32': 'ansi-green',
    '33': 'ansi-yellow',
    '34': 'ansi-blue',
    '35': 'ansi-magenta',
    '36': 'ansi-cyan',
    '37': 'ansi-white',
    '90': 'ansi-bright-black',
    '91': 'ansi-bright-red',
    '92': 'ansi-bright-green',
    '93': 'ansi-bright-yellow',
    '94': 'ansi-bright-blue',
    '95': 'ansi-bright-magenta',
    '96': 'ansi-bright-cyan',
    '97': 'ansi-bright-white',
};

const ansiBgColorMap = {
    '40': 'ansi-bg-black',
    '41': 'ansi-bg-red',
    '42': 'ansi-bg-green',
    '43': 'ansi-bg-yellow',
    '44': 'ansi-bg-blue',
    '45': 'ansi-bg-magenta',
    '46': 'ansi-bg-cyan',
    '47': 'ansi-bg-white',
    '100': 'ansi-bg-bright-black',
    '101': 'ansi-bg-bright-red',
    '102': 'ansi-bg-bright-green',
    '103': 'ansi-bg-bright-yellow',
    '104': 'ansi-bg-bright-blue',
    '105': 'ansi-bg-bright-magenta',
    '106': 'ansi-bg-bright-cyan',
    '107': 'ansi-bg-bright-white',
};

function parseAnsiToHtml(text) {
    let html = '';
    let currentClasses = [];
    let match;
    let lastIndex = 0;

    while ((match = ansiRegex.exec(text)) !== null) {
        html += text.slice(lastIndex, match.index);

        const codes = match[1].split(';');
        for (const code of codes) {
            if (code === '0') {
                currentClasses = [];
            } else if (ansiColorMap[code]) {
                currentClasses.push(ansiColorMap[code]);
            } else if (ansiBgColorMap[code]) {
                currentClasses.push(ansiBgColorMap[code]);
            }
        }

        if (currentClasses.length > 0) {
            html += `<span class="${currentClasses.join(' ')}">`;
        }

        lastIndex = ansiRegex.lastIndex;
    }

    html += text.slice(lastIndex);

    if (currentClasses.length > 0) {
        html += '</span>';
    }

    return html;
}

function addLog(log) {
    const logEntry = document.createElement('div');
    logEntry.className = 'log-entry';
    logEntry.innerHTML = parseAnsiToHtml(log);
    logContainer.appendChild(logEntry);

    if (logContainer.children.length > 100) {
        logContainer.removeChild(logContainer.firstChild);
    }

    logContainer.scrollTop = logContainer.scrollHeight;
}
