import { Component } from 'react';

export class ErrorBoundary extends Component {
  state = { error: null };

  static getDerivedStateFromError(error) {
    return { error };
  }

  render() {
    if (!this.state.error) return this.props.children;
    return (
      <div className="fatal-error">
        <span>Something broke</span>
        <h1>The distribution site could not render.</h1>
        <p>{this.state.error?.message || 'Reload the page and try again.'}</p>
        <button type="button" onClick={() => location.reload()}>Reload</button>
      </div>
    );
  }
}
