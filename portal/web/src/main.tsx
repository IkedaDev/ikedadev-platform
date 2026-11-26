import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import './styles/tokens.css';
import './styles/ui.css';
import App from './App';
import { ThemeProvider } from './context/ThemeProvider';
import { DrawerProvider, ModalProvider, ToastProvider } from './ui';

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <ThemeProvider>
      <ToastProvider>
        <ModalProvider>
          <DrawerProvider>
            <App />
          </DrawerProvider>
        </ModalProvider>
      </ToastProvider>
    </ThemeProvider>
  </StrictMode>,
);
