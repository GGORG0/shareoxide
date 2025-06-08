document
  .getElementById("shorten-form")
  .addEventListener("submit", async (event) => {
    event.preventDefault();

    const link = document.getElementById("link-input").value.trim();
    const shortlink = document.getElementById("shortlink-input").value.trim();

    const shortcuts = shortlink ? [shortlink] : null;

    if (!link) {
      alert("Please enter a URL to shorten.");
      return;
    }

    try {
      const response = await fetch("/api/link", {
        method: "POST",
        headers: {
          "Content-Type": "application/json",
        },
        body: JSON.stringify({ url: link, shortcuts }),
      });

      if (!response.ok) {
        throw new Error("Failed to shorten URL");
      }

      window.location.reload();
    } catch (error) {
      console.error("Error:", error);
      alert("An error occurred while shortening the URL. Please try again.");
    }
  });
