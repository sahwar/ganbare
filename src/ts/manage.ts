/// <reference path="typings/globals/jquery/index.d.ts" />

function accentuate_kana(word) {

	var empty = '<span class="accent">';
	var middle = '<span class="accent"><img src="/static/images/accent_middle.png">';
	var start = '<span class="accent"><img src="/static/images/accent_start.png">';
	var end = '<span class="accent"><img src="/static/images/accent_end.png" class="accent">';
	var flat_end = '<span class="accent"><img src="/static/images/accent_end_flat.png">';
	var start_end = '<span class="accent"><img src="/static/images/accent_start_end.png">';
	var start_end_flat = '<span class="accent"><img src="/static/images/accent_start_end_flat.png">';
	var start_end_flat_short = '<span class="accent"><img src="/static/images/accent_start_end_flat_short.png">';
	var peak = '<span class="accent"><img src="/static/images/accent_peak.png">';
	
	function isAccentMark(i) {
		return (word.charAt(i) === "*" || word.charAt(i) === "・")
	};

	var accentuated = [""];
	var ended = false;
	for (var i = 0, len = word.length; i < len; i++) {

		if (isAccentMark(i)) {
			continue;
		} else if (word.length === 1) {
			accentuated.push(start_end_flat_short);
		} else if (i === 0 && isAccentMark(i+1)) {
			accentuated.push(start_end);
			ended = true;
		} else if (i === 1 && !ended && isAccentMark(i+1)) {
			accentuated.push(peak);
			ended = true;
		} else if (i === 1 && !ended && i === len-1) {
			accentuated.push(start_end_flat);
		} else if (i === 1 && !ended && isAccentMark(i+1)) {
			accentuated.push(start);
		} else if (i > 1 && !ended && i === len-1) {
			accentuated.push(flat_end);
		} else if (i > 1 && !ended && isAccentMark(i+1)) {
			accentuated.push(middle);
		} else if (i > 1 && !ended && isAccentMark(i+1)) {
			accentuated.push(end);
			ended = true;
		} else {
			accentuated.push(empty);
		}
		accentuated.push(word.charAt(i));
		accentuated.push("</span>");
	}
	return accentuated.join("");
}


$(function() {

var main = $("#main");
var n_list = $("#main ul");
var audioPlayer = $("#audioPlayer");

function drawList(nugget_resp, bundle_resp) {

	function createBundle(id, element) {
		var bundle = audio_bundles[id];
		var bundle_html = $('<div class="bordered weak" style="display: inline-block;">ID '+id+'</div>').appendTo(element);
		bundle.files.forEach(function(file) {
			var audio_button = $('<button class="compact"><img src="/static/images/speaker_teal.png" class="soundicon"></button>').appendTo(bundle_html);
			audio_button.click(function() {
				audioPlayer.prop("src", "/api/audio/"+file.id);
				(<HTMLAudioElement>audioPlayer[0]).play();
			});
		});
	};

	var audio_bundles = {};

	bundle_resp.forEach(function(tuple) {
		var bundle = tuple[0];
		var files = tuple[1];
		bundle.files = files;
		audio_bundles[bundle.id] = bundle;
	});

	nugget_resp.forEach(function(tuple, nugget_index) {

		var nugget = tuple[0];
		var n_item = $('<li><hr></li>').appendTo(n_list);
		$("<h2>Skill nugget: " + nugget.skill_summary + "</h2>").appendTo(n_item);

		var c_list = $("<ul></ul>").appendTo(n_item);

		var words = tuple[1][0];
		var questions = tuple[1][1];
		
		words.forEach(function(word, index) {
			var c_item = $("<li></li>").appendTo(c_list);
			var c_header = $('<h3>Word: ' + accentuate_kana(word.word) + ' (ID '+word.id+')</h3>').appendTo(c_item);

			var id = "n"+nugget_index+"w"+index;
			var c_info = $('<div><label for="'+id+'">public</label></div>').appendTo(c_item);

			var checkbox = $('<input type="checkbox" id="'+id+'">').prependTo(c_info);
			if (word.published) {
				checkbox.prop("checked", true);
			};
			checkbox.change(function() {
				var request= { type: 'PUT', url: null };
				if (this.checked) {
					request.url = '/api/words/'+word.id+'?publish';
				} else {
					request.url = '/api/words/'+word.id+'?unpublish';
				};
				$.ajax(request);
			});

			var c_edit = $('<input type="button" value="show" class="linklike">').appendTo(c_info);

			createBundle(word.audio_bundle, c_info);

			var c_body = $('<section class="bordered" style="margin-bottom: 3em;"></section>').appendTo(c_info).hide();
			var w_word = $('<p class="wordShowKana"></p>').appendTo(c_body).html(accentuate_kana(word.word));
			var w_explanation = $('<div class="wordExplanation"></div>').appendTo(c_body).html(word.explanation);

			function showBody() {
				c_edit.val("Hide").click(function() { c_body.hide(); c_edit.val("Show"); c_edit.click(showBody); });
				c_body.show();
			};

			c_edit.click(showBody);
		});

		questions.forEach(function(tuple, index) {
			var question = tuple[0];
			var answers = tuple[1];

			var c_item = $("<li><h3>Question: " + question.q_name + "</h3></li>").appendTo(c_list);

			var id = "n"+nugget_index+"q"+index;
			var c_info = $("<div><label for=\""+id+"\">public</label></div>").appendTo(c_item);

			var checkbox = $('<input type="checkbox" id="'+id+'">').prependTo(c_info);
			if (question.published) {
				checkbox.prop("checked", true);
			};
			checkbox.change(function() {
				var request= { type: 'PUT', url: null };
				if (this.checked) {
					request.url = '/api/questions/'+question.id+'?publish';
				} else {
					request.url = '/api/questions/'+question.id+'?unpublish';
				};
				$.ajax(request);
			});


			var c_edit = $('<input type="button" value="show" class="linklike">').appendTo(c_info);

			answers.forEach(function(ans) {
				createBundle(ans.q_audio_bundle, c_info);
			});

			var c_body = $('<section class="bordered" style="margin-bottom: 3em;"></section>').appendTo(c_info).hide();
			var q_explanation = $('<p class="questionExplanation"></p>').appendTo(c_body).text(question.q_explanation);
			var q_text = $('<p class="questionText"></p>').appendTo(c_body).text(question.question_text);
			var a_list = $('<div class="answerList"></div>').appendTo(c_body);


			answers.forEach(function(ans) {
				var q_answer = $('<div class="answer bordered weak"></div>').appendTo(a_list);
				var q_bundle = $('<p></p>').appendTo(q_answer);
				createBundle(ans.q_audio_bundle, q_bundle);
				var qa_button = $('<div class="answerButton"></div>').appendTo(q_answer);
				qa_button.html(ans.answer_text);
			});

			function showBody() {
				c_edit.val("Hide").click(function() { c_body.hide(); c_edit.val("Show"); c_edit.click(showBody); });
				c_body.show();
			};

			c_edit.click(showBody);
		});

		if (words.length >= 2 && questions.length === 0) {
			var c_item = $("<li><h3>(No questions)</h3></li>").appendTo(c_list);
			var c_body = $('<div><input type="button" value="autocreate" class="linklike"></div>');
			c_body.appendTo(c_item);

		}
		
	});
};

var nugget_resp = null;
var bundle_resp = null;

$.getJSON("/api/bundles", function(resp) {
	bundle_resp = resp;
	if (nugget_resp !== null && bundle_resp !== null) { drawList(nugget_resp, bundle_resp); }
});

$.getJSON("/api/nuggets", function(resp) {
	nugget_resp = resp;
	if (nugget_resp !== null && bundle_resp !== null) { drawList(nugget_resp, bundle_resp); }
});

});
