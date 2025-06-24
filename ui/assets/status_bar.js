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

function setOnlineClientCount(n) {
    const span = document.getElementById("online-clients");
    span.textContent = n;

    span.classList.remove("pulse-counter");
    void span.offsetWidth;
    span.classList.add("pulse-counter");
}
