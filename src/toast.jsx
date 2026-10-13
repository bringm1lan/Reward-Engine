import { createContext, useCallback, useContext, useMemo, useRef, useState } from 'react';
import { CheckCircle2, Info, X, XCircle } from 'lucide-react';

const ToastContext = createContext(null);
const icons = { success: CheckCircle2, error: XCircle, info: Info };

export function ToastProvider({ children }) {
  const [toasts, setToasts] = useState([]);
  const nextId = useRef(0);

  const dismiss = useCallback((id) => {
    setToasts((current) => current.filter((toast) => toast.id !== id));
  }, []);

  const notify = useCallback(({ title, description, variant = 'info' }) => {
    const id = nextId.current++;
    setToasts((current) => [...current, { id, title, description, variant }]);
    if (variant !== 'error') setTimeout(() => dismiss(id), 6_000);
    return id;
  }, [dismiss]);

  const value = useMemo(() => ({ notify, dismiss }), [dismiss, notify]);
  return (
    <ToastContext.Provider value={value}>
      {children}
      <div className="toast-viewport" role="region" aria-live="polite" aria-label="Notifications">
        {toasts.map((toast) => {
          const Icon = icons[toast.variant] ?? Info;
          return (
            <div className={`toast toast-${toast.variant}`} key={toast.id}>
              <Icon size={18} />
              <div><strong>{toast.title}</strong>{toast.description && <p>{toast.description}</p>}</div>
              <button type="button" onClick={() => dismiss(toast.id)} aria-label="Dismiss"><X size={15} /></button>
            </div>
          );
        })}
      </div>
    </ToastContext.Provider>
  );
}

export function useToast() {
  const value = useContext(ToastContext);
  if (!value) throw new Error('useToast must be used within ToastProvider');
  return value;
}
