/**
 * Progressive enhancement only, served from `/assets/app.<hash>.js`.
 * Every page works with JavaScript disabled: filters are real GET forms.
 */
export const JS = `
(function () {
  "use strict";

  // Instant client-side narrowing of any table that declares a filter input.
  document.querySelectorAll("[data-filter-for]").forEach(function (input) {
    var table = document.getElementById(input.getAttribute("data-filter-for"));
    if (!table) return;
    var status = document.getElementById(input.getAttribute("data-filter-status") || "");
    var rows = Array.prototype.slice.call(table.tBodies[0] ? table.tBodies[0].rows : []);
    var submit = input.form ? input.form.querySelector("[data-filter-submit]") : null;
    if (submit) submit.hidden = true;
    input.addEventListener("input", function () {
      var q = input.value.trim().toLowerCase();
      var shown = 0;
      rows.forEach(function (row) {
        var hit = q === "" || row.textContent.toLowerCase().indexOf(q) !== -1;
        row.hidden = !hit;
        if (hit) shown++;
      });
      if (status) {
        status.textContent = q === ""
          ? rows.length + " listed"
          : shown + " of " + rows.length + " match \\u201C" + input.value.trim() + "\\u201D";
      }
    });
  });

  // Submit-on-change for select filters so the server-side form needs no button.
  document.querySelectorAll("form[data-autosubmit] select").forEach(function (sel) {
    sel.addEventListener("change", function () {
      if (sel.form) sel.form.submit();
    });
  });
})();
`;
