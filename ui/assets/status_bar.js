const statusBar = document.getElementById('footer-status-bar');
const statusDot = statusBar.querySelector('.status-dot');
const statusText = statusBar.querySelector('.status-text');

function setOffline() {
    statusDot.style.backgroundColor = 'red';
    statusText.textContent = 'Offline';
}

function setOnline() {
    statusDot.style.backgroundColor = 'green';
    statusText.textContent = 'Online';
}
