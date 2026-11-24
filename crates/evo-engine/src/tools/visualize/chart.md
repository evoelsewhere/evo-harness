
## Chart.js Integration

### Setup
```html
<script src="https://cdn.jsdelivr.net/npm/chart.js"></script>
```

### Canvas Wrapper
```css
.chart-container {
  position: relative;
  height: 300px;
  width: 100%;
}
```

### Basic Chart
A canvas cannot read `var(--…)`: read each color with `themeColor(name)`
(provided by Evo) and redraw on the `evo:themechange` event, which fires when
the person changes the theme, accent or background.
```javascript
const ctx = document.getElementById('chart').getContext('2d');
const chart = new Chart(ctx, {
  type: 'bar', // line, pie, doughnut, etc.
  data: {
    labels: ['Jan', 'Feb', 'Mar'],
    datasets: [{ label: 'Sales', data: [10, 20, 30] }]
  },
  options: {
    responsive: true,
    maintainAspectRatio: false,
    plugins: {
      legend: { display: false } // Build custom HTML legend
    }
  }
});
function paint() {
  Chart.defaults.color = themeColor('--color-text-secondary');
  Chart.defaults.borderColor = themeColor('--color-border');
  chart.data.datasets[0].backgroundColor = themeColor('--color-accent');
  chart.update();
}
paint();
window.addEventListener('evo:themechange', paint);
```

### Number Formatting
- Currency: `-$5M` not `$-5M`
- Percentage: `42%` not `0.42`
- Large numbers: `1.2K`, `3.4M`

### Dashboard Layout
```css
.dashboard {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(200px, 1fr));
  gap: 1rem;
}
.metric-card {
  background: var(--color-background);
  border: 1px solid var(--color-border);
  border-radius: 8px;
  padding: 1rem;
  text-align: center;
}
.metric-value {
  font-size: 2rem;
  font-weight: 500;
  color: var(--color-accent);
}
.metric-label {
  color: var(--color-text-secondary);
  font-size: 0.875rem;
}
```

### Chart Colors
Use the accent color palette (in canvas code through `themeColor(...)`):
- Primary: `--color-accent`
- Secondary: `--color-success`
- Tertiary: `--color-warning`
- Quaternary: `--color-error`

### Dark Mode, accent and background
These variables follow the app's theme, accent and background color. HTML and
SVG using `var(--…)` update by themselves; canvas charts must repaint on
`evo:themechange` as above. Never hardcode colors.
