// Mobile nav toggle
document.querySelector('.nav-toggle')?.addEventListener('click', function() {
    document.querySelector('.nav-links')?.classList.toggle('active');
});

// Copy code button
function copyCode(btn) {
    const code = btn.parentElement.querySelector('code').textContent;
    navigator.clipboard.writeText(code).then(() => {
        const original = btn.innerHTML;
        btn.innerHTML = '<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M20 6L9 17l-5-5"/></svg>';
        setTimeout(() => btn.innerHTML = original, 2000);
    });
}

// Smooth scroll for anchor links
document.querySelectorAll('a[href^="#"]').forEach(anchor => {
    anchor.addEventListener('click', function(e) {
        e.preventDefault();
        const target = document.querySelector(this.getAttribute('href'));
        if (target) target.scrollIntoView({ behavior: 'smooth' });
    });
});

// Animate elements on scroll
const observer = new IntersectionObserver((entries) => {
    entries.forEach(entry => {
        if (entry.isIntersecting) {
            entry.target.classList.add('animate-in');
        }
    });
}, { threshold: 0.1 });

document.querySelectorAll('.feature-card, .step').forEach(el => observer.observe(el));
