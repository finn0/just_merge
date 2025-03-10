// 获取状态栏元素
const statusBar = document.getElementById('footer-status-bar');
const statusDot = statusBar.querySelector('.status-dot');
const statusText = statusBar.querySelector('.status-text');

// 更新状态为离线
function setOffline() {
    statusDot.style.backgroundColor = 'red'; // 红色圆点
    statusText.textContent = 'Offline'; // 更新文本
}

// 更新状态为在线
function setOnline() {
    statusDot.style.backgroundColor = 'green'; // 绿色圆点
    statusText.textContent = 'Online'; // 更新文本
}

// 示例：5 秒后切换为离线状态
setTimeout(setOffline, 5000);
