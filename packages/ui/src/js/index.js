// @plbls/ui — web components et comportements. Importer ce module suffit :
// les balises `plbls-*` sont enregistrées, les infobulles et la barre latérale
// sont actives.
import { installBehaviors } from './behaviors.js';

export { PlblsModal } from './components/modal.js';
export { PlblsDropdownMenu } from './components/dropdown-menu.js';
export { PlblsTabs } from './components/tabs.js';
export { PlblsToastContainer, toast } from './components/toast.js';
export { PlblsOtpInput } from './components/otp-input.js';
export { PlblsCopyField } from './components/copy-field.js';
export { PlblsPasswordStrength, passwordScore } from './components/password-strength.js';
export { PlblsPagination } from './components/pagination.js';
export { PlblsDataTable } from './components/data-table.js';
export { PlblsDropzone } from './components/dropzone.js';
export { getTheme, setTheme, initTheme } from './theme.js';
export { installBehaviors };

installBehaviors();
