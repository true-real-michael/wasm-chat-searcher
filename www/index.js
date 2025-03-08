import "./index.css";
import {Searcher} from "wasm-chat-searcher";

function run() {
    const fileInput = document.getElementById("file-input");
    const loading = document.getElementById("loading");
    const uploadSection = document.getElementById("upload-section");
    const searchDisplaySection = document.getElementById(
        "search-display-section",
    );
    const threadsList = document.getElementById("threads-list");
    const detailsContent = document.getElementById("details-content");
    const searchInput = document.getElementById("search-input");
    const searchButton = document.getElementById("search-button");

    let currentThreadId = null; // Track the currently displayed thread ID
    let currentDetails = []; // Store the currently displayed messages
    const messagesPerPage = 5; // Number of messages to load per "Load More" click

    const searcher = Searcher.new();

    // 2. Handle file upload
    fileInput.addEventListener("change", async (event) => {
        const file = event.target.files[0];
        if (file) {
            // Show loading indicator
            loading.classList.remove("hidden");

            // Read the file as text
            const fileContent = await file.text();

            // 3. Set data
            try {
                searcher.set_data(fileContent);
                fileInput.classList.add("hidden");
                loading.classList.add("hidden");
                uploadSection.classList.add("hidden");
                searchDisplaySection.classList.remove("hidden");
            } catch (error) {
                console.error("Error setting data:", error);
                alert(
                    "Error: The uploaded file is not in the correct format.  Please ensure it's valid JSON.",
                );
                loading.classList.add("hidden");
                fileInput.value = ""; // Clear the file input
            }
        }
    });

    // Function to update threads based on the search query
    function updateThreads() {
        const query = searchInput.value.toLowerCase();
        const threads = searcher.find_threads(query);
        renderThreads(threads);
    }

    // Handle search button click
    searchButton.addEventListener("click", () => {
        updateThreads();
    });

    // Handle Enter key press in the search input
    searchInput.addEventListener("keypress", (event) => {
        if (event.key === "Enter") {
            updateThreads();
        }
    });

    // Render threads in the left column
    function renderThreads(threads) {
        threadsList.innerHTML = threads
            .map(
                (thread) => {
                    const date = new Date(thread.date_unixtime * 1000);
                    const year = date.getFullYear();
                    const month = String(date.getMonth() + 1).padStart(2, "0");
                    const formattedDate = `${year}-${month}`;
                    return `
                        <li class="p-2 hover:bg-gray-100 cursor-pointer" data-id="${thread.thread_id}">
                            <div class="flex justify-between items-center">
                                <span class="truncate">${thread.title_text}</span>
                                <span class="text-sm text-gray-500 ml-2 whitespace-nowrap">${formattedDate}</span>
                            </div>
                        </li>
                    `
                },
            )
            .join("");

        // Add click event listeners to threads
        document.querySelectorAll("#threads-list li").forEach((thread) => {
            thread.addEventListener("click", () => {
                const threadId = thread.getAttribute("data-id");
                currentThreadId = threadId; // Store the current thread ID
                loadInitialDetails(threadId);
            });
        });
    }

    function loadInitialDetails(threadId) {
        currentDetails = searcher.get_thread_messages(threadId);
        renderDetails(currentDetails);
    }

    // Load more messages before the current set
    function loadMoreBefore() {
        if (!currentThreadId) return;
        const firstMessageId = currentDetails[0].message_id;
        const newMessages = searcher.get_message_range(
            firstMessageId - messagesPerPage - 1,
            firstMessageId - 1,
        );
        currentDetails = [...newMessages, ...currentDetails];
        renderDetails(currentDetails);
    }

    // Load more messages after the current set
    function loadMoreAfter() {
        if (!currentThreadId) return;
        const lastMessageId =
            currentDetails[currentDetails.length - 1].message_id;
        const newMessages = searcher.get_message_range(
            lastMessageId + 1,
            lastMessageId + messagesPerPage + 1,
        );
        currentDetails = [...currentDetails, ...newMessages];
        renderDetails(currentDetails);
    }

    // Render details in the right column
    function renderDetails(
        details,
        hasMoreBefore = true,
        hasMoreAfter = true,
    ) {
        let html = "";

        if (hasMoreBefore) {
            html +=
                '<button id="load-before" class="bg-gray-200 p-2 rounded mb-2 w-full">Load More Before</button>';
        }

        html += details
            .map(
                (message) => `
        <div class="bg-blue-100 rounded-lg p-2 m-2">
            ${
                    message.reply_to_text
                        ? `
                <div class="truncate bg-blue-200 p-1 ml-1">
                    ${message.reply_to_text}
                </div>
            `
                        : ""
                }
            <div>${message.text}</div>
        </div>
    `,
            )
            .join("");

        if (hasMoreAfter) {
            html +=
                '<button id="load-after" class="bg-gray-200 p-2 rounded mt-2 w-full">Load More After</button>';
        }

        detailsContent.innerHTML = html;

        // Attach event listeners to the "Load More" buttons
        const loadBeforeButton = document.getElementById("load-before");
        if (loadBeforeButton) {
            loadBeforeButton.addEventListener("click", loadMoreBefore);
        }

        const loadAfterButton = document.getElementById("load-after");
        if (loadAfterButton) {
            loadAfterButton.addEventListener("click", loadMoreAfter);
        }
    }
}

run();
