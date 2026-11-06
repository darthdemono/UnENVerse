// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
interface Window {
  __unvBulkToggle?: (index: number) => boolean;
  __unvIsBulkMode?: () => boolean;
}

declare module '*.txt?raw' {
  const text: string;
  export default text;
}
