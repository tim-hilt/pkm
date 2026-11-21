// Runs in Safari's page, so that what the user sees is Saved, logins included, and without a
// connection. Scripts and styles are left out: they are most of a large page and no text.
var ExtensionPreprocessingJS = {
    run: function (arguments) {
        var page = document.documentElement.cloneNode(true);
        page.querySelectorAll("script, style, template").forEach(function (node) {
            node.remove();
        });
        arguments.completionFunction({ url: document.URL, html: page.outerHTML });
    },
};
